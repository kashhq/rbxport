//! Controller events to engine calls (fast path) or bridge events (slow path).
//!
//! [`decide`] is pure: an event, the host's state and the time in, an
//! [`Outcome`] out, so every routing rule is a unit test. [`apply`] is the one
//! place an outcome touches the engine or the webview. The split follows
//! `docs/perform/design.md` §Routing: jog, the tempo slider, the mixer strip,
//! the cue bus and key shift are fast; everything with a beat-grid or UI
//! opinion (PLAY included, for quantize) is slow.

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use rbl_controller::{
    Button, Deck, DeckControl, Event, Flx4, GlobalControl, JogSurface, KeyShiftPad, PadMode,
};
use rbl_deck::{Band, Engine, Snapshot};
use tauri::{AppHandle, Emitter};

use super::dto::{ButtonDto, GlobalButtonDto, PerformEventDto};

/// The event the bridge listens to (`src/ipc/perform.ts`).
pub const EVENT: &str = "perform:event";

/// The TEMPO slider's range either side of centre: rekordbox's default ±6 %.
/// The UI's own range setting (±6/10/16/WIDE) lives in the frontend and is
/// not reachable from Rust today, so the slider always spans ±6 %; SHIFT+SYNC
/// cycling the range in the UI does not widen it here.
pub const TEMPO_RANGE: f32 = 0.06;

/// Platter travel per jog tick, in track milliseconds: 720 ticks a turn at
/// 33⅓ rpm is 1.8 s / 720 = 2.5 ms (flx4-midi §2.2, the Mixxx mapping's
/// `scratchEnable(720, 33⅓)`; `[OBS mapping]`, not measured on a unit).
pub const MS_PER_TICK: f64 = 2.5;

/// Pitch bend per jog tick, as a fraction of the base tempo: a slow nudge of
/// a tick or two is a fraction of a percent, a hard spin a few percent.
/// `[ASSUME]` tuned by feel against Mixxx's `delta × 0.8` jog, not measured.
pub const BEND_PER_TICK: f32 = 0.004;

/// A bend holds this long after the last tick, then the base tempo returns.
pub const BEND_HOLD: Duration = Duration::from_millis(80);

/// SHIFT + platter: fast search, track milliseconds per tick (flx4-midi §2.2).
pub const SEARCH_MS_PER_TICK: f64 = 150.0;

/// Full scale of a 14-bit control, and the TEMPO slider's centre.
const FULL_SCALE: u16 = 0x3FFF;
const CENTRE: u16 = 0x2000;

/// The 14-bit deck controls, in the order `DeckState::controls` stores them.
const DECK_CONTROLS: [DeckControl; 7] = [
    DeckControl::Tempo,
    DeckControl::Trim,
    DeckControl::EqHigh,
    DeckControl::EqMid,
    DeckControl::EqLow,
    DeckControl::ChannelFader,
    DeckControl::Filter,
];

const GLOBAL_CONTROLS: [GlobalControl; 2] = [GlobalControl::Crossfader, GlobalControl::HeadphoneMix];

/// An engine call the host makes itself, without the webview.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FastOp {
    Tempo { deck: Deck, ratio: f32 },
    Trim { deck: Deck, gain: f32 },
    Band { deck: Deck, band: Band, position: f32 },
    /// The channel fader, 0 to 1.
    Level { deck: Deck, level: f32 },
    /// 0 is deck A alone, 1 is deck B alone.
    Crossfade(f32),
    /// 0 is the cue sum alone, 1 is the master alone.
    HeadphoneMix(f32),
    Pfl { deck: Deck, on: bool },
    ScrubBegin(Deck),
    ScrubTo { deck: Deck, ms: f64 },
    ScrubEnd(Deck),
    /// Relative: moves the playhead by `ms` from wherever the clock is.
    Search { deck: Deck, ms: f64 },
    KeyShift { deck: Deck, semitones: i8 },
}

impl FastOp {
    /// Knobs, faders and jog ticks, which arrive at MIDI rate: logged at
    /// trace, where buttons are logged at debug.
    fn continuous(self) -> bool {
        !matches!(
            self,
            Self::Pfl { .. } | Self::ScrubBegin(_) | Self::ScrubEnd(_) | Self::KeyShift { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Fast(FastOp),
    Slow(PerformEventDto),
    Ignore,
}

/// Controls milestone 1 does not handle (`docs/perform/design.md`). Each is
/// logged once per session at debug, so a log shows they arrived.
#[derive(Debug, Clone, Copy)]
enum Unhandled {
    Filter,
    FxLevel,
    FxChannel,
    BeatFx,
    SamplerPads,
    KeyboardPads,
    PadFxPads,
}

impl Unhandled {
    const COUNT: usize = 7;

    fn index(self) -> usize {
        self as usize
    }
}

/// A hand on the platter, and where it has dragged the track to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JogDrag {
    pub position_ms: f64,
}

/// A pitch bend in progress: the tempo to return to, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    pub base: f32,
    pub until: Instant,
}

/// What the engine said about a deck, read just before an event is decided.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckView {
    pub loaded: bool,
    /// Negative inside the pre-roll before the track's start.
    pub position_ms: f64,
    pub tempo: f32,
    pub key_shift: i8,
}

impl DeckView {
    fn of(deck: &rbl_deck::DeckSnapshot, sample_rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "a track is far under 2^52 frames")]
        let frames = deck.position_frames as f64 - deck.pre_roll_frames as f64;
        let position_ms = if sample_rate == 0 { 0.0 } else { frames * 1000.0 / f64::from(sample_rate) };
        Self { loaded: deck.loaded, position_ms, tempo: deck.tempo, key_shift: deck.key_shift }
    }
}

#[derive(Debug, Default)]
pub struct DeckState {
    /// The deck's SHIFT button. The hardware shifts its own buttons; this is
    /// for controls it does not (flx4-midi §5.5).
    pub shift_held: bool,
    /// The channel CUE button, toggled by the host: the cue bus is ours.
    pub pfl: bool,
    /// The page the pads are on, from the last mode button pressed. The
    /// controller switches pages itself (flx4-midi §4); this only drives LEDs.
    pub pad_mode: Option<PadMode>,
    pub jog: Option<JogDrag>,
    pub bend: Option<Bend>,
    /// Last raw value per `DECK_CONTROLS` entry, to replay onto a new engine.
    controls: [Option<u16>; DECK_CONTROLS.len()],
    pub view: DeckView,
}

/// Which engine the host last pushed its state into. Holds a `Weak` rather
/// than an address: the allocation stays reserved while this lives, so a new
/// engine can never reuse the old one's address and look unchanged.
pub struct Attached<T>(Weak<T>);

impl<T> std::fmt::Debug for Attached<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Attached").field(&self.0.as_ptr()).finish()
    }
}

impl<T> Default for Attached<T> {
    fn default() -> Self {
        Self(Weak::new())
    }
}

impl<T> Attached<T> {
    /// Records `current` and says whether it differs from the last one seen.
    /// No engine is not a change: the next one to appear is.
    pub fn changed(&mut self, current: Option<&Arc<T>>) -> bool {
        let Some(current) = current else { return false };
        if std::ptr::eq(self.0.as_ptr(), Arc::as_ptr(current)) {
            return false;
        }
        self.0 = Arc::downgrade(current);
        true
    }
}

/// The host's own state, behind `Perform::host`: touched only from the MIDI
/// callback and the output thread.
#[derive(Debug, Default)]
pub struct HostState {
    /// Indexed `[A, B]`.
    pub decks: [DeckState; 2],
    /// Last raw value per `GLOBAL_CONTROLS` entry.
    globals: [Option<u16>; GLOBAL_CONTROLS.len()],
    logged: [bool; Unhandled::COUNT],
    engine: Attached<Engine>,
}

impl HostState {
    pub fn deck(&self, deck: Deck) -> &DeckState {
        match deck {
            Deck::A => &self.decks[0],
            Deck::B => &self.decks[1],
        }
    }

    fn deck_mut(&mut self, deck: Deck) -> &mut DeckState {
        match deck {
            Deck::A => &mut self.decks[0],
            Deck::B => &mut self.decks[1],
        }
    }

    /// Takes the engine's current view of both decks; no engine is two
    /// empty decks.
    pub fn observe(&mut self, snapshot: Option<&Snapshot>) {
        let [a, b] = &mut self.decks;
        if let Some(s) = snapshot {
            a.view = DeckView::of(&s.a, s.sample_rate);
            b.view = DeckView::of(&s.b, s.sample_rate);
        } else {
            a.view = DeckView::default();
            b.view = DeckView::default();
        }
    }

    /// Notes the engine the next ops go to. When it is a new one (built
    /// late, or rebuilt by a device or wish change) a drag or bend in flight
    /// belonged to the old one, and the caller replays [`Self::replay`].
    pub fn attach(&mut self, engine: Option<&Arc<Engine>>) -> bool {
        if !self.engine.changed(engine) {
            return false;
        }
        for deck in &mut self.decks {
            deck.jog = None;
            deck.bend = None;
        }
        true
    }

    /// Everything the controller has set, as ops: a fresh engine starts with
    /// the mixer and cue bus at their defaults, not where the hardware is.
    pub fn replay(&self) -> Vec<FastOp> {
        let mut ops = Vec::new();
        for deck in Deck::ALL {
            let state = self.deck(deck);
            for (control, value) in DECK_CONTROLS.into_iter().zip(state.controls) {
                if let Some(op) = value.and_then(|value| deck_op(deck, control, value)) {
                    ops.push(op);
                }
            }
            if state.pfl {
                ops.push(FastOp::Pfl { deck, on: true });
            }
        }
        for (control, value) in GLOBAL_CONTROLS.into_iter().zip(self.globals) {
            if let Some(value) = value {
                ops.push(global_op(control, value));
            }
        }
        ops
    }

    /// Bends whose hold has run out, ended: each returns its deck to the base.
    pub fn expire_bends(&mut self, now: Instant) -> Vec<FastOp> {
        let mut ops = Vec::new();
        for (deck, state) in Deck::ALL.into_iter().zip(&mut self.decks) {
            if let Some(bend) = state.bend.filter(|bend| bend.until <= now) {
                state.bend = None;
                ops.push(FastOp::Tempo { deck, ratio: bend.base });
            }
        }
        ops
    }

    /// The controller went away: end what a hand was doing, take the decks
    /// out of the headphones, and forget the hardware's positions (a new
    /// connection dumps them again, flx4-midi §5.1).
    pub fn release(&mut self) -> Vec<FastOp> {
        let mut ops = Vec::new();
        for (deck, state) in Deck::ALL.into_iter().zip(&mut self.decks) {
            if state.jog.take().is_some() {
                ops.push(FastOp::ScrubEnd(deck));
            }
            if let Some(bend) = state.bend.take() {
                ops.push(FastOp::Tempo { deck, ratio: bend.base });
            }
            if state.pfl {
                ops.push(FastOp::Pfl { deck, on: false });
            }
            *state = DeckState { view: state.view, ..DeckState::default() };
        }
        self.globals = [None; GLOBAL_CONTROLS.len()];
        ops
    }

    fn unhandled(&mut self, kind: Unhandled) -> Outcome {
        if let Some(logged) = self.logged.get_mut(kind.index()) {
            if !*logged {
                *logged = true;
                tracing::debug!(?kind, "perform: control not handled in milestone 1");
            }
        }
        Outcome::Ignore
    }
}

/// 0..=0x3FFF as 0..=1.
fn unit(value: u16) -> f32 {
    f32::from(value.min(FULL_SCALE)) / f32::from(FULL_SCALE)
}

/// The TEMPO slider as a speed ratio: 0x0000 is +range, centre is 1, 0x3FFF
/// is just short of −range (flx4-midi §2.3: the raw value is inverted).
pub fn tempo_ratio(value: u16) -> f32 {
    let centre = f32::from(CENTRE);
    1.0 + TEMPO_RANGE * (centre - f32::from(value.min(FULL_SCALE))) / centre
}

fn deck_op(deck: Deck, control: DeckControl, value: u16) -> Option<FastOp> {
    Some(match control {
        DeckControl::Tempo => FastOp::Tempo { deck, ratio: tempo_ratio(value) },
        // 0 to 2, unity at centre: the strip's trim is up to +6 dB.
        DeckControl::Trim => FastOp::Trim { deck, gain: unit(value) * 2.0 },
        DeckControl::EqHigh => FastOp::Band { deck, band: Band::High, position: unit(value) },
        DeckControl::EqMid => FastOp::Band { deck, band: Band::Mid, position: unit(value) },
        DeckControl::EqLow => FastOp::Band { deck, band: Band::Low, position: unit(value) },
        DeckControl::ChannelFader => FastOp::Level { deck, level: unit(value) },
        // The strip has no filter yet (design.md: an `FLX:` addition to come).
        DeckControl::Filter => return None,
    })
}

/// The crossfader's raw 0 is its left end, deck 1 (flx4-midi §2.6), which is
/// the mixer's 0 (deck A). HEADPHONES MIX's left end is CUE, the cue bus's 0.
fn global_op(control: GlobalControl, value: u16) -> FastOp {
    match control {
        GlobalControl::Crossfader => FastOp::Crossfade(unit(value)),
        GlobalControl::HeadphoneMix => FastOp::HeadphoneMix(unit(value)),
    }
}

/// Routes one controller event. `now` stamps a pitch bend's hold.
pub fn decide(event: Event, state: &mut HostState, now: Instant) -> Outcome {
    match event {
        Event::Button { deck, button, shift, pressed } => decide_button(state, deck, button, shift, pressed),
        Event::Pad { deck, mode, pad, shift, pressed } => decide_pad(state, deck, mode, pad, shift, pressed),
        Event::Jog { deck, surface, delta, shift } => decide_jog(state.deck_mut(deck), deck, surface, delta, shift, now),
        Event::Deck14 { deck, control, value } => {
            let index = DECK_CONTROLS.iter().position(|c| *c == control);
            let state_deck = state.deck_mut(deck);
            if let Some(slot) = index.and_then(|i| state_deck.controls.get_mut(i)) {
                *slot = Some(value);
            }
            match deck_op(deck, control, value) {
                Some(op) => {
                    // The slider moved under a bend: the bend now returns here.
                    if let (FastOp::Tempo { ratio, .. }, Some(bend)) = (op, state_deck.bend.as_mut()) {
                        bend.base = ratio;
                    }
                    Outcome::Fast(op)
                }
                None => state.unhandled(Unhandled::Filter),
            }
        }
        Event::Global14 { control, value } => {
            if let Some(slot) = GLOBAL_CONTROLS.iter().position(|c| *c == control).and_then(|i| state.globals.get_mut(i)) {
                *slot = Some(value);
            }
            Outcome::Fast(global_op(control, value))
        }
        Event::Browse { delta, shift } => Outcome::Slow(PerformEventDto::Browse { delta, shift }),
        Event::Global { button, shift, pressed } => match GlobalButtonDto::of(button) {
            Some(button) => Outcome::Slow(PerformEventDto::Global { button, shift, pressed }),
            None => state.unhandled(Unhandled::BeatFx),
        },
        Event::FxLevel { .. } => state.unhandled(Unhandled::FxLevel),
        Event::FxChannel { .. } => state.unhandled(Unhandled::FxChannel),
    }
}

fn decide_button(state: &mut HostState, deck: Deck, button: Button, shift: bool, pressed: bool) -> Outcome {
    let deck_state = state.deck_mut(deck);
    match button {
        Button::Shift => {
            deck_state.shift_held = pressed;
            Outcome::Ignore
        }
        Button::PadMode(mode) => {
            if pressed {
                deck_state.pad_mode = Some(mode);
                tracing::debug!(?deck, ?mode, "perform: pad mode");
            }
            Outcome::Ignore
        }
        Button::JogTouch => jog_touch(deck_state, deck, shift, pressed),
        Button::Pfl if !shift => {
            if !pressed {
                return Outcome::Ignore;
            }
            deck_state.pfl = !deck_state.pfl;
            Outcome::Fast(FastOp::Pfl { deck, on: deck_state.pfl })
        }
        _ => match ButtonDto::of(button) {
            Some(button) => Outcome::Slow(PerformEventDto::Button { deck: deck.into(), button, shift, pressed }),
            None => Outcome::Ignore,
        },
    }
}

/// A hand on the platter starts a scrub from where the deck is. The shifted
/// touch (0x67) does not: SHIFT + platter is fast search, and a scrub would
/// fight the seeks. A release always ends a drag, shifted or not, since
/// SHIFT may have gone down mid-drag.
fn jog_touch(state: &mut DeckState, deck: Deck, shift: bool, pressed: bool) -> Outcome {
    if !pressed {
        return match state.jog.take() {
            Some(_) => Outcome::Fast(FastOp::ScrubEnd(deck)),
            None => Outcome::Ignore,
        };
    }
    if shift || !state.view.loaded || state.jog.is_some() {
        return Outcome::Ignore;
    }
    state.jog = Some(JogDrag { position_ms: state.view.position_ms });
    Outcome::Fast(FastOp::ScrubBegin(deck))
}

fn decide_jog(state: &mut DeckState, deck: Deck, surface: JogSurface, delta: i8, shift: bool, now: Instant) -> Outcome {
    match (surface, shift, state.jog.as_mut()) {
        // Touched: the platter is the record. Shifted ticks too, if SHIFT
        // went down after the hand did.
        (JogSurface::Platter, _, Some(drag)) => {
            drag.position_ms += f64::from(delta) * MS_PER_TICK;
            Outcome::Fast(FastOp::ScrubTo { deck, ms: drag.position_ms })
        }
        (JogSurface::Platter, true, None) if state.view.loaded => {
            Outcome::Fast(FastOp::Search { deck, ms: f64::from(delta) * SEARCH_MS_PER_TICK })
        }
        (JogSurface::Platter, true, None) => Outcome::Ignore,
        // The outer ring, or the platter turned without a hand on top (the
        // touch note is the only reliable signal, flx4-midi §5.8): bend.
        (JogSurface::Side, ..) | (JogSurface::Platter, false, None) => {
            if !state.view.loaded {
                return Outcome::Ignore;
            }
            let base = state.bend.map_or(state.view.tempo, |bend| bend.base);
            state.bend = Some(Bend { base, until: now + BEND_HOLD });
            Outcome::Fast(FastOp::Tempo { deck, ratio: base * (1.0 + f32::from(delta) * BEND_PER_TICK) })
        }
    }
}

fn decide_pad(state: &mut HostState, deck: Deck, mode: PadMode, pad: u8, shift: bool, pressed: bool) -> Outcome {
    match mode {
        PadMode::KeyShift => {
            if !pressed {
                return Outcome::Ignore;
            }
            let current = state.deck(deck).view.key_shift;
            match Flx4::key_shift_pad(pad, shift) {
                Some(KeyShiftPad::Set(semitones)) => Outcome::Fast(FastOp::KeyShift { deck, semitones }),
                Some(KeyShiftPad::Step(step)) => {
                    Outcome::Fast(FastOp::KeyShift { deck, semitones: current.saturating_add(step) })
                }
                None => Outcome::Ignore,
            }
        }
        PadMode::HotCue | PadMode::BeatJump | PadMode::BeatLoop => {
            Outcome::Slow(PerformEventDto::Pad { deck: deck.into(), mode: mode.into(), pad, shift, pressed })
        }
        PadMode::Sampler => state.unhandled(Unhandled::SamplerPads),
        PadMode::Keyboard => state.unhandled(Unhandled::KeyboardPads),
        PadMode::PadFx1 | PadMode::PadFx2 => state.unhandled(Unhandled::PadFxPads),
    }
}

pub fn engine_deck(deck: Deck) -> rbl_deck::Deck {
    match deck {
        Deck::A => rbl_deck::Deck::A,
        Deck::B => rbl_deck::Deck::B,
    }
}

/// The deck's index in the mixer's channels and the cue bus's PFL.
fn index(deck: Deck) -> usize {
    match deck {
        Deck::A => 0,
        Deck::B => 1,
    }
}

/// Makes one fast-path call on the engine.
pub fn apply_fast(op: FastOp, engine: &Engine) {
    if op.continuous() {
        tracing::trace!(?op, "perform: fast path");
    } else {
        tracing::debug!(?op, "perform: fast path");
    }
    match op {
        FastOp::Tempo { deck, ratio } => engine.set_tempo(engine_deck(deck), ratio),
        FastOp::Trim { deck, gain } => {
            if let Some(channel) = engine.mixer().channels.get(index(deck)) {
                channel.set_trim(gain);
            }
        }
        FastOp::Band { deck, band, position } => {
            if let Some(channel) = engine.mixer().channels.get(index(deck)) {
                channel.set_band(band, position);
            }
        }
        FastOp::Level { deck, level } => {
            if let Some(channel) = engine.mixer().channels.get(index(deck)) {
                channel.set_level(level);
            }
        }
        FastOp::Crossfade(position) => engine.mixer().set_crossfade(position),
        FastOp::HeadphoneMix(mix) => engine.cue().set_mix(mix),
        FastOp::Pfl { deck, on } => engine.cue().set_pfl(index(deck), on),
        FastOp::ScrubBegin(deck) => engine.scrub_begin(engine_deck(deck)),
        FastOp::ScrubTo { deck, ms } => engine.scrub_to_ms(engine_deck(deck), ms),
        FastOp::ScrubEnd(deck) => engine.scrub_end(engine_deck(deck)),
        FastOp::Search { deck, ms } => engine.move_ms(engine_deck(deck), ms),
        FastOp::KeyShift { deck, semitones } => engine.set_key_shift(engine_deck(deck), semitones),
    }
}

/// Carries out a decision. A fast op with no engine is dropped: its value is
/// already in `HostState` and is replayed when the engine is built.
pub fn apply(outcome: Outcome, engine: Option<&Engine>, app: &AppHandle) {
    match outcome {
        Outcome::Fast(op) => {
            if let Some(engine) = engine {
                apply_fast(op, engine);
            }
        }
        Outcome::Slow(event) => {
            tracing::debug!(?event, "perform: slow path");
            if let Err(error) = app.emit(EVENT, event) {
                tracing::warn!(%error, "perform: an event did not reach the bridge");
            }
        }
        Outcome::Ignore => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[allow(clippy::float_cmp, reason = "the scalings' ends are exact, and that is the assertion")]
mod tests {
    use rbl_controller::{FxChannel, GlobalButton};

    use super::super::dto::{DeckDto, PadModeDto};
    use super::*;

    fn loaded() -> HostState {
        let mut state = HostState::default();
        for deck in &mut state.decks {
            deck.view = DeckView { loaded: true, position_ms: 1_000.0, tempo: 1.0, key_shift: 0 };
        }
        state
    }

    fn run(state: &mut HostState, event: Event) -> Outcome {
        decide(event, state, Instant::now())
    }

    fn button(deck: Deck, button: Button, shift: bool, pressed: bool) -> Event {
        Event::Button { deck, button, shift, pressed }
    }

    fn deck14(deck: Deck, control: DeckControl, value: u16) -> Event {
        Event::Deck14 { deck, control, value }
    }

    fn jog(deck: Deck, surface: JogSurface, delta: i8, shift: bool) -> Event {
        Event::Jog { deck, surface, delta, shift }
    }

    fn fast(outcome: &Outcome) -> FastOp {
        match outcome {
            Outcome::Fast(op) => *op,
            other => panic!("expected a fast op, got {other:?}"),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_tempo_slider_spans_six_percent_inverted() {
        assert_eq!(tempo_ratio(0), 1.06);
        assert_eq!(tempo_ratio(0x2000), 1.0);
        assert!(close(tempo_ratio(0x3FFF), 0.94), "{}", tempo_ratio(0x3FFF));
        let mut state = loaded();
        assert_eq!(
            fast(&run(&mut state, deck14(Deck::B, DeckControl::Tempo, 0))),
            FastOp::Tempo { deck: Deck::B, ratio: 1.06 }
        );
    }

    #[test]
    fn mixer_controls_scale_at_both_ends_and_centre() {
        let mut state = loaded();
        let op = |state: &mut HostState, control, value| fast(&run(state, deck14(Deck::A, control, value)));
        assert_eq!(op(&mut state, DeckControl::Trim, 0), FastOp::Trim { deck: Deck::A, gain: 0.0 });
        assert_eq!(op(&mut state, DeckControl::Trim, 0x3FFF), FastOp::Trim { deck: Deck::A, gain: 2.0 });
        let FastOp::Trim { gain, .. } = op(&mut state, DeckControl::Trim, 0x2000) else { panic!() };
        assert!(close(gain, 1.0), "trim centre was {gain}");
        assert_eq!(
            op(&mut state, DeckControl::EqHigh, 0x3FFF),
            FastOp::Band { deck: Deck::A, band: Band::High, position: 1.0 }
        );
        assert_eq!(
            op(&mut state, DeckControl::EqMid, 0),
            FastOp::Band { deck: Deck::A, band: Band::Mid, position: 0.0 }
        );
        let FastOp::Band { band: Band::Low, position, .. } = op(&mut state, DeckControl::EqLow, 0x2000) else { panic!() };
        assert!(close(position, 0.5), "EQ centre was {position}");
        assert_eq!(op(&mut state, DeckControl::ChannelFader, 0), FastOp::Level { deck: Deck::A, level: 0.0 });
        assert_eq!(op(&mut state, DeckControl::ChannelFader, 0x3FFF), FastOp::Level { deck: Deck::A, level: 1.0 });
        assert_eq!(run(&mut state, deck14(Deck::A, DeckControl::Filter, 0x1000)), Outcome::Ignore);
    }

    #[test]
    fn crossfader_and_headphone_mix_are_unit_scaled() {
        let mut state = loaded();
        let global = |state: &mut HostState, control, value| fast(&run(state, Event::Global14 { control, value }));
        assert_eq!(global(&mut state, GlobalControl::Crossfader, 0), FastOp::Crossfade(0.0));
        assert_eq!(global(&mut state, GlobalControl::Crossfader, 0x3FFF), FastOp::Crossfade(1.0));
        assert_eq!(global(&mut state, GlobalControl::HeadphoneMix, 0x3FFF), FastOp::HeadphoneMix(1.0));
        let FastOp::HeadphoneMix(mix) = global(&mut state, GlobalControl::HeadphoneMix, 0x2000) else { panic!() };
        assert!(close(mix, 0.5));
    }

    #[test]
    fn pfl_toggles_on_press_and_shift_pfl_goes_to_the_bridge() {
        let mut state = loaded();
        assert_eq!(fast(&run(&mut state, button(Deck::A, Button::Pfl, false, true))), FastOp::Pfl { deck: Deck::A, on: true });
        assert_eq!(run(&mut state, button(Deck::A, Button::Pfl, false, false)), Outcome::Ignore);
        assert!(state.decks[0].pfl);
        assert_eq!(fast(&run(&mut state, button(Deck::A, Button::Pfl, false, true))), FastOp::Pfl { deck: Deck::A, on: false });
        assert!(!state.decks[0].pfl);
        assert_eq!(
            run(&mut state, button(Deck::B, Button::Pfl, true, true)),
            Outcome::Slow(PerformEventDto::Button { deck: DeckDto::B, button: ButtonDto::Pfl, shift: true, pressed: true })
        );
    }

    #[test]
    fn transport_buttons_go_to_the_bridge_with_their_releases() {
        let cases = [
            (Button::Play, ButtonDto::Play),
            (Button::Cue, ButtonDto::Cue),
            (Button::Sync, ButtonDto::Sync),
            (Button::SyncLong, ButtonDto::SyncLong),
            (Button::LoopIn, ButtonDto::LoopIn),
            (Button::LoopOut, ButtonDto::LoopOut),
            (Button::Reloop, ButtonDto::Reloop),
            (Button::CueLoopCallBack, ButtonDto::CallBack),
            (Button::CueLoopCallForward, ButtonDto::CallForward),
        ];
        let mut state = loaded();
        for (input, wire) in cases {
            for (shift, pressed) in [(false, true), (false, false), (true, true), (true, false)] {
                assert_eq!(
                    run(&mut state, button(Deck::A, input, shift, pressed)),
                    Outcome::Slow(PerformEventDto::Button { deck: DeckDto::A, button: wire, shift, pressed }),
                    "{input:?} shift={shift} pressed={pressed}"
                );
            }
        }
    }

    #[test]
    fn shift_and_pad_mode_buttons_update_state_only() {
        let mut state = loaded();
        assert_eq!(run(&mut state, button(Deck::B, Button::Shift, false, true)), Outcome::Ignore);
        assert!(state.decks[1].shift_held);
        assert_eq!(run(&mut state, button(Deck::B, Button::Shift, false, false)), Outcome::Ignore);
        assert!(!state.decks[1].shift_held);
        assert_eq!(run(&mut state, button(Deck::A, Button::PadMode(PadMode::KeyShift), true, true)), Outcome::Ignore);
        assert_eq!(state.decks[0].pad_mode, Some(PadMode::KeyShift));
        // A release does not change the page.
        assert_eq!(run(&mut state, button(Deck::A, Button::PadMode(PadMode::HotCue), false, false)), Outcome::Ignore);
        assert_eq!(state.decks[0].pad_mode, Some(PadMode::KeyShift));
    }

    #[test]
    fn a_touched_platter_scrubs_from_where_the_deck_was() {
        let mut state = loaded();
        assert_eq!(fast(&run(&mut state, button(Deck::A, Button::JogTouch, false, true))), FastOp::ScrubBegin(Deck::A));
        assert_eq!(fast(&run(&mut state, jog(Deck::A, JogSurface::Platter, 4, false))), FastOp::ScrubTo { deck: Deck::A, ms: 1_010.0 });
        assert_eq!(fast(&run(&mut state, jog(Deck::A, JogSurface::Platter, -8, false))), FastOp::ScrubTo { deck: Deck::A, ms: 990.0 });
        // SHIFT pressed mid-drag: the shifted ticks still scrub.
        assert_eq!(fast(&run(&mut state, jog(Deck::A, JogSurface::Platter, 2, true))), FastOp::ScrubTo { deck: Deck::A, ms: 995.0 });
        assert_eq!(fast(&run(&mut state, button(Deck::A, Button::JogTouch, true, false))), FastOp::ScrubEnd(Deck::A));
        assert_eq!(state.decks[0].jog, None);
        assert_eq!(run(&mut state, button(Deck::A, Button::JogTouch, false, false)), Outcome::Ignore);
    }

    #[test]
    fn touching_an_empty_deck_or_with_shift_does_not_scrub() {
        let mut state = HostState::default();
        assert_eq!(run(&mut state, button(Deck::A, Button::JogTouch, false, true)), Outcome::Ignore);
        let mut state = loaded();
        assert_eq!(run(&mut state, button(Deck::A, Button::JogTouch, true, true)), Outcome::Ignore);
        assert_eq!(state.decks[0].jog, None);
    }

    #[test]
    fn an_untouched_platter_or_the_ring_bends_and_the_bend_expires() {
        let mut state = loaded();
        state.decks[1].view.tempo = 1.02;
        let start = Instant::now();
        let FastOp::Tempo { deck: Deck::B, ratio } = fast(&decide(jog(Deck::B, JogSurface::Side, 5, false), &mut state, start)) else {
            panic!()
        };
        assert!(close(ratio, 1.02 * (1.0 + 5.0 * BEND_PER_TICK)), "{ratio}");
        // The engine now reports the bent tempo; the bend keeps its base.
        state.decks[1].view.tempo = ratio;
        let FastOp::Tempo { ratio, .. } = fast(&decide(jog(Deck::B, JogSurface::Platter, -2, false), &mut state, start)) else {
            panic!()
        };
        assert!(close(ratio, 1.02 * (1.0 - 2.0 * BEND_PER_TICK)), "{ratio}");
        assert_eq!(state.expire_bends(start + BEND_HOLD / 2), Vec::new());
        assert_eq!(state.expire_bends(start + BEND_HOLD), vec![FastOp::Tempo { deck: Deck::B, ratio: 1.02 }]);
        assert_eq!(state.decks[1].bend, None);
    }

    #[test]
    fn the_tempo_slider_under_a_bend_moves_the_bend_s_base() {
        let mut state = loaded();
        let now = Instant::now();
        decide(jog(Deck::A, JogSurface::Side, 3, false), &mut state, now);
        decide(deck14(Deck::A, DeckControl::Tempo, 0), &mut state, now);
        assert_eq!(state.expire_bends(now + BEND_HOLD), vec![FastOp::Tempo { deck: Deck::A, ratio: 1.06 }]);
    }

    #[test]
    fn shift_platter_searches_and_an_empty_deck_ignores_the_jog() {
        let mut state = loaded();
        assert_eq!(fast(&run(&mut state, jog(Deck::A, JogSurface::Platter, -3, true))), FastOp::Search { deck: Deck::A, ms: -450.0 });
        let mut state = HostState::default();
        assert_eq!(run(&mut state, jog(Deck::A, JogSurface::Platter, -3, true)), Outcome::Ignore);
        assert_eq!(run(&mut state, jog(Deck::A, JogSurface::Side, 1, false)), Outcome::Ignore);
        assert_eq!(state.decks[0].bend, None);
    }

    #[test]
    fn key_shift_pads_set_unshifted_and_step_shifted() {
        let mut state = loaded();
        let pad = |pad, shift, pressed| Event::Pad { deck: Deck::B, mode: PadMode::KeyShift, pad, shift, pressed };
        let expected_set = [0, 1, 2, 3, -3, -2, -1, 0];
        for (n, semitones) in (0..8).zip(expected_set) {
            assert_eq!(fast(&run(&mut state, pad(n, false, true))), FastOp::KeyShift { deck: Deck::B, semitones });
        }
        state.decks[1].view.key_shift = 2;
        let expected_step = [3, 4, 5, 6, -2, -1, 0, 1];
        for (n, semitones) in (0..8).zip(expected_step) {
            assert_eq!(fast(&run(&mut state, pad(n, true, true))), FastOp::KeyShift { deck: Deck::B, semitones });
        }
        assert_eq!(run(&mut state, pad(0, false, false)), Outcome::Ignore);
    }

    #[test]
    fn grid_pads_go_to_the_bridge_and_milestone_two_pads_do_not() {
        let mut state = loaded();
        for (mode, wire) in [
            (PadMode::HotCue, PadModeDto::HotCue),
            (PadMode::BeatJump, PadModeDto::BeatJump),
            (PadMode::BeatLoop, PadModeDto::BeatLoop),
        ] {
            for (shift, pressed) in [(false, true), (true, false)] {
                assert_eq!(
                    run(&mut state, Event::Pad { deck: Deck::A, mode, pad: 3, shift, pressed }),
                    Outcome::Slow(PerformEventDto::Pad { deck: DeckDto::A, mode: wire, pad: 3, shift, pressed })
                );
            }
        }
        for mode in [PadMode::Sampler, PadMode::Keyboard, PadMode::PadFx1, PadMode::PadFx2] {
            assert_eq!(run(&mut state, Event::Pad { deck: Deck::A, mode, pad: 0, shift: false, pressed: true }), Outcome::Ignore);
        }
    }

    #[test]
    fn browse_load_and_beat_fx() {
        let mut state = loaded();
        assert_eq!(
            run(&mut state, Event::Browse { delta: -1, shift: false }),
            Outcome::Slow(PerformEventDto::Browse { delta: -1, shift: false })
        );
        let global = |button, shift, pressed| Event::Global { button, shift, pressed };
        assert_eq!(
            run(&mut state, global(GlobalButton::Load(Deck::B), false, true)),
            Outcome::Slow(PerformEventDto::Global { button: GlobalButtonDto::LoadB, shift: false, pressed: true })
        );
        assert_eq!(
            run(&mut state, global(GlobalButton::Load(Deck::A), false, false)),
            Outcome::Slow(PerformEventDto::Global { button: GlobalButtonDto::LoadA, shift: false, pressed: false })
        );
        assert_eq!(
            run(&mut state, global(GlobalButton::BrowsePress, true, true)),
            Outcome::Slow(PerformEventDto::Global { button: GlobalButtonDto::BrowsePress, shift: true, pressed: true })
        );
        for button in [GlobalButton::BeatFxSelect, GlobalButton::BeatFxLeft, GlobalButton::BeatFxRight, GlobalButton::BeatFxOnOff] {
            assert_eq!(run(&mut state, global(button, false, true)), Outcome::Ignore);
        }
        assert_eq!(run(&mut state, Event::FxLevel { value: 64 }), Outcome::Ignore);
        assert_eq!(run(&mut state, Event::FxChannel { channel: FxChannel::Two, selected: true }), Outcome::Ignore);
    }

    #[test]
    fn a_new_engine_gets_every_stored_value_replayed() {
        let mut state = loaded();
        run(&mut state, deck14(Deck::A, DeckControl::ChannelFader, 0));
        run(&mut state, deck14(Deck::A, DeckControl::ChannelFader, 0x3FFF));
        run(&mut state, deck14(Deck::B, DeckControl::Tempo, 0x2000));
        run(&mut state, deck14(Deck::B, DeckControl::Filter, 0x2000));
        run(&mut state, button(Deck::B, Button::Pfl, false, true));
        run(&mut state, Event::Global14 { control: GlobalControl::Crossfader, value: 0 });
        assert_eq!(
            state.replay(),
            vec![
                FastOp::Level { deck: Deck::A, level: 1.0 },
                FastOp::Tempo { deck: Deck::B, ratio: 1.0 },
                FastOp::Pfl { deck: Deck::B, on: true },
                FastOp::Crossfade(0.0),
            ]
        );
    }

    #[test]
    fn attaching_tracks_engine_identity_not_address() {
        let mut attached = Attached::<u8>::default();
        assert!(!attached.changed(None), "no engine is not a change");
        let first = Arc::new(1_u8);
        assert!(attached.changed(Some(&first)));
        assert!(!attached.changed(Some(&first)));
        assert!(!attached.changed(None));
        assert!(!attached.changed(Some(&Arc::clone(&first))), "the same engine through another handle");
        drop(first);
        // The weak handle keeps the old allocation reserved, so the new
        // engine cannot land on its address and pass for it.
        let second = Arc::new(2_u8);
        assert!(attached.changed(Some(&second)));
    }

    #[test]
    fn release_ends_drags_bends_and_pfl_and_forgets_positions() {
        let mut state = loaded();
        let now = Instant::now();
        decide(button(Deck::A, Button::JogTouch, false, true), &mut state, now);
        decide(jog(Deck::B, JogSurface::Side, 1, false), &mut state, now);
        decide(button(Deck::B, Button::Pfl, false, true), &mut state, now);
        decide(deck14(Deck::A, DeckControl::Trim, 0), &mut state, now);
        decide(Event::Global14 { control: GlobalControl::HeadphoneMix, value: 0 }, &mut state, now);
        assert_eq!(
            state.release(),
            vec![FastOp::ScrubEnd(Deck::A), FastOp::Tempo { deck: Deck::B, ratio: 1.0 }, FastOp::Pfl { deck: Deck::B, on: false }]
        );
        assert_eq!(state.replay(), Vec::new());
        assert!(state.decks[1].view.loaded, "the engine's view is not the controller's to forget");
    }
}
