//! The DDJ-FLX4's MIDI ports: discovery, hot-plug, keep-alive and LED output.
//!
//! The same shape as `rx3_link.rs`: port names compacted before matching,
//! a named thread that owns the output and refreshes the controller's lease,
//! a send error taken as the device gone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use rbl_controller::{Flx4, Leds};
use tauri::AppHandle;

use super::leds::LedFrames;
use super::Perform;

/// How often to look for the controller, and to check it is still there.
/// Port enumeration only, no I/O (`docs/perform/design.md` §Hot-plug).
const POLL: Duration = Duration::from_secs(2);
/// `perform-out`'s beat: the bend restore's granularity.
const TICK: Duration = Duration::from_millis(10);
const LED_EVERY: Duration = Duration::from_millis(50);
const KEEPALIVE_EVERY: Duration = Duration::from_millis(Flx4::KEEPALIVE_INTERVAL_MS);
const CLIENT: &str = "rbxport perform";
/// The map every matched port is driven with. One controller today; a
/// second is a second name test and map here.
const MAP: &str = "ddj-flx4";

/// Starts `perform-watch`, which runs for the life of the app.
pub fn spawn(app: AppHandle, perform: Arc<Perform>) {
    let spawned = std::thread::Builder::new()
        .name("perform-watch".to_owned())
        .spawn(move || watch(&app, &perform));
    if let Err(error) = spawned {
        tracing::error!(%error, "perform: the controller watcher could not be started");
    }
}

fn watch(app: &AppHandle, perform: &Arc<Perform>) {
    let mut session: Option<Session> = None;
    // The last port list logged, so an unchanged list is not logged every poll.
    let mut seen: Vec<String> = Vec::new();
    loop {
        session = match session.take() {
            None => connect(app, perform, &mut seen),
            Some(session) => match session.lost() {
                Some(reason) => {
                    session.close(app, perform, reason);
                    None
                }
                None => Some(session),
            },
        };
        std::thread::sleep(POLL);
    }
}

/// The FLX4's ports as the OS names them: "DDJ-FLX4" on macOS and Windows,
/// "DDJ-FLX4:DDJ-FLX4 MIDI 1 24:0" under ALSA `[ASSUME]` (not yet seen).
fn is_flx4_port_name(name: &str) -> bool {
    let compact: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect();
    compact.contains("DDJFLX4")
}

fn input_names() -> Result<Vec<String>, midir::InitError> {
    let input = MidiInput::new(CLIENT)?;
    Ok(input.ports().iter().filter_map(|port| input.port_name(port).ok()).collect())
}

/// One live connection.
struct Session {
    name: String,
    input: MidiInputConnection<()>,
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    output: JoinHandle<()>,
}

fn connect(app: &AppHandle, perform: &Arc<Perform>, seen: &mut Vec<String>) -> Option<Session> {
    let output = MidiOutput::new(CLIENT)
        .inspect_err(|error| tracing::warn!(%error, "perform: MIDI outputs unavailable"))
        .ok()?;
    let mut input = MidiInput::new(CLIENT)
        .inspect_err(|error| tracing::warn!(%error, "perform: MIDI inputs unavailable"))
        .ok()?;
    // SysEx (the controller's answer to the keep-alive), clock and active
    // sensing decode to nothing; keep them off the callback.
    input.ignore(Ignore::All);

    let out_port = output.ports().into_iter().find(|port| output.port_name(port).is_ok_and(|n| is_flx4_port_name(&n)));
    let in_port = input.ports().into_iter().find(|port| input.port_name(port).is_ok_and(|n| is_flx4_port_name(&n)));
    let (Some(out_port), Some(in_port)) = (out_port, in_port) else {
        let names: Vec<String> = input.ports().iter().filter_map(|port| input.port_name(port).ok()).collect();
        if names != *seen {
            tracing::debug!(inputs = ?names, "perform: no DDJ-FLX4 among the MIDI inputs");
            *seen = names;
        }
        return None;
    };
    let name = input.port_name(&in_port).ok()?;

    let out_connection = output
        .connect(&out_port, "rbxport perform out")
        .inspect_err(|error| tracing::warn!(%error, port = %name, "perform: could not open the controller's output"))
        .ok()?;
    let mut decoder = Flx4::new();
    let callback_app = app.clone();
    let callback_perform = Arc::clone(perform);
    let input_connection = input
        .connect(
            &in_port,
            "rbxport perform in",
            move |_stamp, message, ()| {
                if let Some(event) = decoder.decode(message) {
                    callback_perform.handle(event, &callback_app);
                }
            },
            (),
        )
        .inspect_err(|error| tracing::warn!(%error, port = %name, "perform: could not open the controller's input"))
        .ok()?;

    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let output_thread = {
        let (app, perform, stop, failed) = (app.clone(), Arc::clone(perform), Arc::clone(&stop), Arc::clone(&failed));
        std::thread::Builder::new()
            .name("perform-out".to_owned())
            .spawn(move || drive(out_connection, &app, &perform, &stop, &failed))
    };
    let output_thread = match output_thread {
        Ok(handle) => handle,
        Err(error) => {
            tracing::error!(%error, "perform: the controller output thread could not be started");
            return None;
        }
    };
    tracing::info!(port = %name, map = MAP, "perform: controller connected");
    perform.connected(app, &name);
    Some(Session { name, input: input_connection, stop, failed, output: output_thread })
}

impl Session {
    /// Why the connection is over, if it is: a failed send, or the port gone
    /// from the system's list.
    fn lost(&self) -> Option<&'static str> {
        if self.failed.load(Ordering::Relaxed) || self.output.is_finished() {
            return Some("a MIDI send failed");
        }
        match input_names() {
            Ok(names) if !names.contains(&self.name) => Some("the port went away"),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(%error, "perform: could not list MIDI inputs; keeping the connection");
                None
            }
        }
    }

    fn close(self, app: &AppHandle, perform: &Perform, reason: &str) {
        // Input first, so no event lands after the state is released.
        let (_input, ()) = self.input.close();
        self.stop.store(true, Ordering::Relaxed);
        if self.output.join().is_err() {
            tracing::error!("perform: the controller output thread panicked");
        }
        perform.disconnected(app);
        tracing::info!(port = %self.name, reason, "perform: controller disconnected");
    }
}

/// `perform-out`: keep-alive, LED frames and the host's housekeeping, until
/// told to stop or a send fails.
fn drive(mut out: MidiOutputConnection, app: &AppHandle, perform: &Perform, stop: &AtomicBool, failed: &AtomicBool) {
    let mut frames = LedFrames::default();
    let mut keepalive_at: Option<Instant> = None;
    let mut leds_at: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        // The first one doubles as the request for every knob and fader's
        // position (flx4-midi §5.1), which then arrive as ordinary events.
        if keepalive_at.is_none_or(|at| now.duration_since(at) >= KEEPALIVE_EVERY) {
            if let Err(error) = out.send(&Flx4::KEEPALIVE) {
                tracing::warn!(%error, "perform: keep-alive failed, the controller is gone");
                failed.store(true, Ordering::Relaxed);
                return;
            }
            keepalive_at = Some(now);
        }
        perform.service(app, now);
        if leds_at.is_none_or(|at| now.duration_since(at) >= LED_EVERY) {
            for message in perform.led_messages(app, &mut frames) {
                if let Err(error) = out.send(&message) {
                    tracing::warn!(%error, "perform: LED write failed, the controller is gone");
                    failed.store(true, Ordering::Relaxed);
                    return;
                }
            }
            leds_at = Some(now);
        }
        std::thread::sleep(TICK);
    }
    // Best effort: a controller left lit looks connected.
    for message in Leds::blackout() {
        if out.send(&message).is_err() {
            break;
        }
    }
    tracing::debug!("perform: controller output stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flx4_port_names_tolerate_platform_punctuation() {
        assert!(is_flx4_port_name("DDJ-FLX4"));
        assert!(is_flx4_port_name("Pioneer DDJ-FLX4 MIDI"));
        assert!(is_flx4_port_name("DDJ-FLX4:DDJ-FLX4 MIDI 1 24:0"));
        assert!(is_flx4_port_name("ddj flx4"));
        assert!(!is_flx4_port_name("DDJ-400"));
        assert!(!is_flx4_port_name("DDJ-FLX6"));
        assert!(!is_flx4_port_name("XDJ-RX3"));
        assert!(!is_flx4_port_name("Midi Through:Midi Through Port-0 14:0"));
    }

    #[test]
    fn the_output_beat_fits_the_keep_alive_and_the_bend() {
        assert_eq!(KEEPALIVE_EVERY, Duration::from_millis(200));
        assert!(TICK * 2 <= super::super::route::BEND_HOLD / 4);
        assert!(TICK < LED_EVERY);
    }
}
