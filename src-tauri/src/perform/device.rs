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
use rbl_deck::{AudioDevice, Engine};
use tauri::{AppHandle, Manager};

use crate::player::Player;

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

/// The FLX4's MIDI ports and its sound card as the OS names them: "DDJ-FLX4"
/// on macOS and Windows, "DDJ-FLX4:DDJ-FLX4 MIDI 1 24:0" under ALSA
/// `[ASSUME]` (neither the ALSA name nor the audio name seen on a unit yet).
fn is_flx4_name(name: &str) -> bool {
    let compact: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect();
    compact.contains("DDJFLX4")
}

/// Whether the output the `Player` opens is the FLX4's own sound card, the
/// only device whose lanes 3–4 are known to be headphones. A chosen id that
/// is no longer plugged in opens the default, as `CpalSink::open_named` does.
fn cue_wanted(chosen: Option<&str>, devices: &[AudioDevice], default: Option<&AudioDevice>) -> bool {
    let opened = chosen.and_then(|id| devices.iter().find(|device| device.id == id)).or(default);
    opened.is_some_and(|device| is_flx4_name(&device.name))
}

/// Keeps `StreamWish::cue` following the audio output while a controller is
/// connected: checked on connect, whenever the engine is rebuilt, and when
/// Preferences picks another device (`Player::device` changes at once; the
/// engine only on the next play). Enumerating devices is the expensive part,
/// so nothing is enumerated on a tick where neither changed.
#[derive(Default)]
struct CueGate {
    checked: bool,
    /// `Player::device` at the last check.
    device: Option<String>,
}

impl CueGate {
    fn check(&mut self, app: &AppHandle, engine_changed: bool) {
        let device = app.state::<Arc<Player>>().device();
        if self.checked && !engine_changed && self.device == device {
            return;
        }
        let wanted = cue_wanted(device.as_deref(), &rbl_deck::output_devices(), rbl_deck::default_output_device().as_ref());
        tracing::debug!(?device, wanted, "perform: cue bus checked against the audio output");
        self.checked = true;
        self.device = device;
        super::set_cue_bus(app, wanted);
    }
}

fn same_engine(a: Option<&Arc<Engine>>, b: Option<&Arc<Engine>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
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

    let out_port = output.ports().into_iter().find(|port| output.port_name(port).is_ok_and(|n| is_flx4_name(&n)));
    let in_port = input.ports().into_iter().find(|port| input.port_name(port).is_ok_and(|n| is_flx4_name(&n)));
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

/// `perform-out`: keep-alive, LED frames, the engine cache, the cue gate and
/// the host's housekeeping, until told to stop or a send fails. A failed
/// send only ends the connection: the audio wish and the engine stay.
fn drive(mut out: MidiOutputConnection, app: &AppHandle, perform: &Perform, stop: &AtomicBool, failed: &AtomicBool) {
    let mut frames = LedFrames::default();
    let mut keepalive_at: Option<Instant> = None;
    let mut leds_at: Option<Instant> = None;
    let mut gate = CueGate::default();
    // One strong engine while connected, so its last drop lands here rather
    // than on the `CoreMIDI` thread (see `Perform::engine`).
    let mut held: Option<Arc<Engine>> = None;
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
        let current = app.state::<Arc<Player>>().opened();
        let changed = !same_engine(current.as_ref(), held.as_ref());
        let retired = if changed {
            perform.cache(current.as_ref());
            std::mem::replace(&mut held, current)
        } else {
            None
        };
        gate.check(app, changed);
        perform.service(held.as_ref(), now);
        // After `service` took the host lock: a callback that upgraded the
        // old engine has finished with it.
        drop(retired);
        if leds_at.is_none_or(|at| now.duration_since(at) >= LED_EVERY) {
            for message in perform.led_messages(held.as_ref(), &mut frames) {
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
        assert!(is_flx4_name("DDJ-FLX4"));
        assert!(is_flx4_name("Pioneer DDJ-FLX4 MIDI"));
        assert!(is_flx4_name("DDJ-FLX4:DDJ-FLX4 MIDI 1 24:0"));
        assert!(is_flx4_name("ddj flx4"));
        assert!(!is_flx4_name("DDJ-400"));
        assert!(!is_flx4_name("DDJ-FLX6"));
        assert!(!is_flx4_name("XDJ-RX3"));
        assert!(!is_flx4_name("Midi Through:Midi Through Port-0 14:0"));
    }

    fn device(id: &str, name: &str) -> AudioDevice {
        AudioDevice { id: id.to_owned(), name: name.to_owned() }
    }

    #[test]
    fn the_cue_bus_follows_the_output_the_player_opens() {
        let flx4 = device("flx4-id", "DDJ-FLX4");
        let speakers = device("mac-id", "MacBook Pro Speakers");
        let devices = [flx4.clone(), speakers.clone()];
        assert!(cue_wanted(Some("flx4-id"), &devices, Some(&speakers)), "chosen FLX4");
        assert!(!cue_wanted(Some("mac-id"), &devices, Some(&flx4)), "chosen speakers, whatever the default");
        assert!(cue_wanted(None, &devices, Some(&flx4)), "system default is the FLX4");
        assert!(!cue_wanted(None, &devices, Some(&speakers)));
        assert!(cue_wanted(Some("unplugged"), &devices, Some(&flx4)), "a missing choice opens the default");
        assert!(!cue_wanted(None, &devices, None));
    }

    #[test]
    fn the_output_beat_fits_the_keep_alive_and_the_bend() {
        assert_eq!(KEEPALIVE_EVERY, Duration::from_millis(200));
        assert!(TICK * 2 <= super::super::route::BEND_HOLD / 4);
        assert!(TICK < LED_EVERY);
    }
}
