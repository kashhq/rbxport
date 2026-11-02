//! The controller host: the DDJ-FLX4's MIDI connection, the fast path into
//! `rbl_deck`, the slow path to the bridge, and LED feedback.
//!
//! Fork addition (`docs/fork.md`, `docs/perform/design.md`). Threads:
//! - `perform-watch` (`device.rs`): polls for the controller every 2 s,
//!   connects, and notices it going away.
//! - the MIDI input callback (midir's thread): decodes, then [`Perform::handle`].
//! - `perform-out` (`device.rs`): owns the MIDI output. Keep-alive every
//!   200 ms, LED frames every 50 ms, and [`Perform::service`] every 10 ms to
//!   end pitch bends and replay controller state onto a rebuilt engine.
//!
//! `HostState` is shared by the input callback and `perform-out` only, so its
//! lock is never contended by a command; the bridge's report has its own.

mod dto;
mod leds;
mod route;

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod device;

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use rbl_controller::Event;
use rbl_deck::{Engine, StreamWish};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::player::Player;

use dto::{PerformEventDto, PerformLedsDto, PerformStatusDto};
use leds::{LedFrames, UiDeck};
use route::HostState;

#[derive(Default)]
pub struct Perform {
    host: Mutex<HostState>,
    /// What the bridge last reported through `perform_leds`, `[A, B]`.
    ui: Mutex<[UiDeck; 2]>,
    /// The connected controller's port name.
    connected: Mutex<Option<String>>,
}

/// Registers the host and starts watching for a controller. Called from
/// `setup`, before the webview can invoke `perform_*`.
pub fn start(app: &AppHandle) {
    let perform = Arc::new(Perform::default());
    app.manage(Arc::clone(&perform));
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    device::spawn(app.clone(), perform);
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        drop(perform);
        tracing::debug!("perform: no MIDI on this platform");
    }
}

impl Perform {
    /// One decoded controller event, from the MIDI callback.
    pub fn handle(&self, event: Event, app: &AppHandle) {
        let engine = app.state::<Arc<Player>>().opened();
        let mut host = self.host.lock();
        Self::attach(&mut host, engine.as_ref());
        host.observe(engine.as_ref().map(|engine| engine.snapshot()).as_ref());
        let outcome = route::decide(event, &mut host, Instant::now());
        // Under the lock, so `service` cannot end a bend between this
        // decision and its engine call.
        route::apply(outcome, engine.as_deref(), app);
    }

    /// The output thread's housekeeping between events: replay onto a new
    /// engine, and end the bends whose hold has run out.
    pub fn service(&self, app: &AppHandle, now: Instant) {
        let engine = app.state::<Arc<Player>>().opened();
        let mut host = self.host.lock();
        Self::attach(&mut host, engine.as_ref());
        let ended = host.expire_bends(now);
        if let Some(engine) = engine.as_deref() {
            for op in ended {
                route::apply_fast(op, engine);
            }
        }
    }

    /// Replays the controller's positions onto an engine the host has not
    /// seen yet: one built after the controller moved, or rebuilt by a device
    /// or wish change, starts with its mixer and cue bus at their defaults.
    fn attach(host: &mut HostState, engine: Option<&Arc<Engine>>) {
        let Some(engine) = engine else { return };
        if !host.attach(Some(engine)) {
            return;
        }
        let ops = host.replay();
        tracing::info!(ops = ops.len(), "perform: engine (re)built, controller state re-applied");
        for op in ops {
            route::apply_fast(op, engine);
        }
    }

    /// The next LED messages for this connection.
    pub fn led_messages(&self, app: &AppHandle, frames: &mut LedFrames) -> Vec<[u8; 3]> {
        let engine = app.state::<Arc<Player>>().opened();
        let snapshot = engine.as_ref().map(|engine| engine.snapshot());
        // The RMS holds its last value once the stream stops, so a silent
        // engine reads zero rather than whatever was playing last.
        let level = engine.as_ref().filter(|engine| engine.any_sounding()).map_or(0, |engine| {
            let (left, right) = engine.master().rms();
            leds::vu_level(left.max(right))
        });
        let ui = *self.ui.lock();
        let next = leds::compose(snapshot.as_ref(), level, &ui, &self.host.lock());
        frames.next(next, snapshot.as_ref())
    }

    /// A controller is connected: tell the bridge, and open the device's
    /// headphone lanes. Widening drops the engine, as an audio-config change
    /// does, so the interface reloads its decks on `deck:reset`.
    pub fn connected(&self, app: &AppHandle, name: &str) {
        *self.connected.lock() = Some(name.to_owned());
        emit(app, PerformEventDto::Connected { name: name.to_owned() });
        set_cue_bus(app, true);
    }

    /// The controller went away: end what a hand was doing on it, close the
    /// headphone lanes, tell the bridge.
    pub fn disconnected(&self, app: &AppHandle) {
        let engine = app.state::<Arc<Player>>().opened();
        let ops = self.host.lock().release();
        if let Some(engine) = engine.as_deref() {
            for op in ops {
                route::apply_fast(op, engine);
            }
        }
        *self.connected.lock() = None;
        set_cue_bus(app, false);
        emit(app, PerformEventDto::Disconnected);
    }
}

fn set_cue_bus(app: &AppHandle, cue: bool) {
    let player = app.state::<Arc<Player>>();
    if player.set_wish(StreamWish { cue, ..player.wish() }) {
        tracing::info!(cue, "perform: audio output reopened for the cue bus");
        if let Err(error) = app.emit("deck:reset", ()) {
            tracing::warn!(%error, "perform: deck:reset did not reach the interface");
        }
    }
}

fn emit(app: &AppHandle, event: PerformEventDto) {
    if let Err(error) = app.emit(route::EVENT, event) {
        tracing::warn!(%error, "perform: an event did not reach the bridge");
    }
}

/// `perform_status`: the connected controller's port name, or null.
/// Synchronous: one uncontended lock, nothing that waits.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, reason = "Tauri hands the state over by value")]
pub fn perform_status(perform: State<'_, Arc<Perform>>) -> PerformStatusDto {
    PerformStatusDto { connected: perform.connected.lock().clone() }
}

/// `perform_leds`: the bridge's LED-relevant UI state, sent on change. The
/// next LED frame picks it up.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, reason = "Tauri hands the state over by value")]
pub fn perform_leds(perform: State<'_, Arc<Perform>>, leds: PerformLedsDto) {
    let report = leds::ui_decks(leds);
    tracing::trace!(?report, "perform: LED report from the bridge");
    *perform.ui.lock() = report;
}
