//! LED feedback: the engine's deck state, the bridge's report and the host's
//! own state, composed into one `rbl_controller::Leds` frame and diffed
//! against the last one sent.

use rbl_controller::{DeckLeds, Leds, PadMode};
use rbl_deck::{DeckSnapshot, Snapshot};

use super::dto::{PerformDeckLedsDto, PerformLedsDto};
use super::route::HostState;

/// The meter's floor: an RMS this far under full scale lights nothing.
/// `[ASSUME]` a 40 dB span reads like a mixer's meter; not matched to
/// rekordbox's on the FLX4.
const VU_FLOOR_DB: f32 = -40.0;

/// What the bridge last reported for one deck (`perform_leds`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiDeck {
    pub hot_cues: [bool; 8],
    pub sync: bool,
    pub cue: bool,
    pub pad_mode: Option<PadMode>,
}

impl From<PerformDeckLedsDto> for UiDeck {
    /// A `hotCues` that is not eight long is read pad by pad; the missing
    /// pads are dark and the extra ones dropped.
    fn from(dto: PerformDeckLedsDto) -> Self {
        let mut hot_cues = [false; 8];
        for (slot, set) in hot_cues.iter_mut().zip(dto.hot_cues) {
            *slot = set;
        }
        Self { hot_cues, sync: dto.sync, cue: dto.cue, pad_mode: dto.pad_mode.map(PadMode::from) }
    }
}

/// Both decks, `[A, B]`.
pub fn ui_decks(dto: PerformLedsDto) -> [UiDeck; 2] {
    [dto.a.into(), dto.b.into()]
}

/// The VU level, 0..=127, from an RMS amplitude.
pub fn vu_level(rms: f32) -> u8 {
    if rms.is_nan() || rms <= 0.0 {
        return 0;
    }
    let db = 20.0 * rms.min(1.0).log10();
    let scaled = ((db - VU_FLOOR_DB) / -VU_FLOOR_DB).clamp(0.0, 1.0) * 127.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=127 above")]
    let level = scaled.round() as u8;
    level
}

fn deck_leds(snapshot: Option<&DeckSnapshot>, ui: &UiDeck, host_pfl: bool, host_mode: Option<PadMode>, level: u8) -> DeckLeds {
    // No engine is two empty decks.
    let fact = |read: fn(&DeckSnapshot) -> bool| snapshot.is_some_and(read);
    let looping = fact(|d| d.looping);
    DeckLeds {
        play: fact(|d| d.playing),
        cue: ui.cue,
        sync: ui.sync,
        pfl: host_pfl,
        // The engine knows a loop is running, not how the UI got there; all
        // three loop buttons light for it (flx4-midi §3.1 blinks them).
        loop_in: looping,
        loop_out: looping,
        reloop: looping,
        // The last mode button pressed wins; before any press, the page the
        // UI says it is showing.
        pad_mode: host_mode.or(ui.pad_mode),
        hot_cues: ui.hot_cues,
        // Which loop size is active is not reported by the engine or the UI.
        beat_loops: [false; 8],
        key_shift: DeckLeds::key_shift_pads(snapshot.map_or(0, |d| d.key_shift)),
        loaded: fact(|d| d.loaded),
        level,
    }
}

/// One frame from everything the LEDs follow. `level` is the VU for both
/// decks: the engine meters only the master (`Master::rms`), so both meters
/// show the master's level, as a per-deck meter is not exposed.
pub fn compose(snapshot: Option<&Snapshot>, level: u8, ui: &[UiDeck; 2], host: &HostState) -> Leds {
    let [ui_a, ui_b] = ui;
    let [host_a, host_b] = &host.decks;
    Leds {
        decks: [
            deck_leds(snapshot.map(|s| &s.a), ui_a, host_a.pfl, host_a.pad_mode, level),
            deck_leds(snapshot.map(|s| &s.b), ui_b, host_b.pfl, host_b.pad_mode, level),
        ],
        beat_fx: false,
    }
}

/// The frames sent so far on one connection.
#[derive(Debug, Default)]
pub struct LedFrames {
    previous: Option<Leds>,
    load_ids: [u64; 2],
}

impl LedFrames {
    /// The messages that move the hardware to `next`; the whole frame on
    /// the first call. A new load on a deck that already showed one blanks
    /// `loaded` for this frame, so the next frame's write retriggers the
    /// jog-ring animation: the diff alone never resends an unchanged LED
    /// (`docs/decisions.md`, "FLX4 LEDs render as a fixed frame and diff").
    pub fn next(&mut self, mut next: Leds, snapshot: Option<&Snapshot>) -> Vec<[u8; 3]> {
        if let Some(snapshot) = snapshot {
            for (i, deck) in [&snapshot.a, &snapshot.b].into_iter().enumerate() {
                let (Some(seen), Some(leds)) = (self.load_ids.get_mut(i), next.decks.get_mut(i)) else { continue };
                if deck.load_id == *seen {
                    continue;
                }
                *seen = deck.load_id;
                let was_lit = self.previous.as_ref().and_then(|p| p.decks.get(i)).is_some_and(|d| d.loaded);
                if was_lit && leds.loaded {
                    leds.loaded = false;
                }
            }
        }
        let messages = next.messages(self.previous.as_ref());
        self.previous = Some(next);
        messages
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn snapshot(load_a: u64) -> Snapshot {
        let empty = rbl_deck::DeckClock::default().snapshot();
        let a = DeckSnapshot { loaded: load_a != 0, load_id: load_a, ..empty };
        Snapshot { a, b: empty, sample_rate: 44_100 }
    }

    fn frame(snapshot: &Snapshot) -> Leds {
        compose(Some(snapshot), 0, &[UiDeck::default(); 2], &HostState::default())
    }

    /// The jog-ring write for deck 1: note-on Ch 16, note 0 (flx4-midi §3.3).
    fn ring(messages: &[[u8; 3]]) -> Option<u8> {
        messages.iter().find(|m| m[0] == 0x9F && m[1] == 0x00).map(|m| m[2])
    }

    #[test]
    fn a_second_load_retriggers_the_jog_ring_animation() {
        let mut frames = LedFrames::default();
        let first = snapshot(1);
        assert_eq!(ring(&frames.next(frame(&first), Some(&first))), Some(0x7F));
        assert_eq!(ring(&frames.next(frame(&first), Some(&first))), None, "nothing changed");
        let second = snapshot(2);
        assert_eq!(ring(&frames.next(frame(&second), Some(&second))), Some(0x00), "blanked for one frame");
        assert_eq!(ring(&frames.next(frame(&second), Some(&second))), Some(0x7F), "and lit again");
    }

    #[test]
    fn a_first_load_lights_the_ring_without_a_blank_frame() {
        let mut frames = LedFrames::default();
        let empty = snapshot(0);
        frames.next(frame(&empty), Some(&empty));
        let loaded = snapshot(5);
        assert_eq!(ring(&frames.next(frame(&loaded), Some(&loaded))), Some(0x7F));
    }

    #[test]
    fn the_vu_spans_forty_decibels() {
        assert_eq!(vu_level(0.0), 0);
        assert_eq!(vu_level(f32::NAN), 0);
        assert_eq!(vu_level(0.01), 0, "-40 dB is the floor");
        assert_eq!(vu_level(1.0), 127);
        assert_eq!(vu_level(4.0), 127);
        assert_eq!(vu_level(0.1), 64, "-20 dB is half way");
    }

    #[test]
    fn the_host_s_pad_mode_wins_over_the_ui_s() {
        let ui = [UiDeck { pad_mode: Some(PadMode::HotCue), hot_cues: [true; 8], ..UiDeck::default() }, UiDeck::default()];
        let mut host = HostState::default();
        assert_eq!(compose(None, 0, &ui, &host).decks[0].pad_mode, Some(PadMode::HotCue));
        host.decks[0].pad_mode = Some(PadMode::KeyShift);
        host.decks[0].pfl = true;
        let leds = compose(None, 0, &ui, &host);
        assert_eq!(leds.decks[0].pad_mode, Some(PadMode::KeyShift));
        assert!(leds.decks[0].pfl);
        assert_eq!(leds.decks[0].key_shift, DeckLeds::key_shift_pads(0));
    }

    #[test]
    fn a_short_hot_cue_report_is_read_pad_by_pad() {
        let ui = UiDeck::from(PerformDeckLedsDto { hot_cues: vec![true, true], ..PerformDeckLedsDto::default() });
        assert_eq!(ui.hot_cues, [true, true, false, false, false, false, false, false]);
    }
}
