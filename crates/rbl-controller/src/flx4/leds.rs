//! FLX4 LED model and renderer. flx4-midi §3, §5.4
//!
//! The renderer writes a fixed frame: every LED address the host drives, in
//! the same order whatever the state. Diffing two frames element by element
//! then gives exactly the messages that changed, including the clears when the
//! pad mode changes (the old mode's pad notes drop to off in the new frame).

use crate::{Deck, PadMode};

use super::map;

/// One deck's LED state, filled by the host from deck state and the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per on/off LED; the hardware has no other states (flx4-midi §3)"
)]
pub struct DeckLeds {
    pub play: bool,
    pub cue: bool,
    pub sync: bool,
    pub pfl: bool,
    pub loop_in: bool,
    pub loop_out: bool,
    pub reloop: bool,
    /// The mode shown on the pads. Pad LEDs below are written for this mode
    /// only; `None` lights no pad-mode button and no pads.
    pub pad_mode: Option<PadMode>,
    pub hot_cues: [bool; 8],
    pub beat_loops: [bool; 8],
    /// The §4 semitone bitmap, pad 1 first.
    pub key_shift: [bool; 8],
    /// Track loaded: drives the jog-ring animation (flx4-midi §3.3).
    pub loaded: bool,
    /// VU level 0..=127; higher values are clamped.
    pub level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Leds {
    /// Indexed by deck: `[A, B]`.
    pub decks: [DeckLeds; 2],
    pub beat_fx: bool,
}

/// Addresses per deck: 12 transport + 8 mode buttons + VU + jog ring
/// + 2 pad channels × 3 modes × 8 pads.
const DECK_FRAME_LEN: usize = 12 + 8 + 1 + 1 + 2 * 3 * 8;
/// Both decks plus the two beat FX addresses.
const FRAME_LEN: usize = 2 * DECK_FRAME_LEN + 2;

impl Leds {
    /// Every message that moves the hardware from `previous` to `self`.
    /// `None` writes every LED (startup, or after a reconnect).
    pub fn messages(&self, previous: Option<&Leds>) -> Vec<[u8; 3]> {
        let Some(previous) = previous else {
            return self.frame();
        };
        if previous == self {
            return Vec::new();
        }
        self.frame()
            .into_iter()
            .zip(previous.frame())
            .filter_map(|(next, prev)| (next != prev).then_some(next))
            .collect()
    }

    /// Everything dark, for shutdown: every LED this renderer drives, plus
    /// the sampler pads Mixxx clears on all four pad channels (flx4-midi §3.5).
    pub fn blackout() -> Vec<[u8; 3]> {
        let mut out = Leds::default().frame();
        for deck in Deck::ALL {
            for channel in map::pad_channels(deck) {
                for pad in 0..map::PAD_COUNT {
                    out.push([
                        map::NOTE_ON | channel,
                        map::PAD_BASE_SAMPLER + pad,
                        map::LED_OFF,
                    ]);
                }
            }
        }
        out
    }

    fn frame(&self) -> Vec<[u8; 3]> {
        let mut out = Vec::with_capacity(FRAME_LEN);
        for (deck, leds) in Deck::ALL.into_iter().zip(&self.decks) {
            leds.write(deck, &mut out);
        }
        // The LED is driven on Ch 5 only, even when FX inputs arrive on Ch 6.
        // flx4-midi §3.4
        let fx = map::NOTE_ON | map::CH_FX_1;
        out.push([fx, map::BEAT_FX_ON_OFF, led(self.beat_fx)]);
        out.push([fx, map::SHIFT_BEAT_FX_ON_OFF, led(self.beat_fx)]);
        out
    }
}

impl DeckLeds {
    /// The key-shift pads' bitmap for a deck shifted `semitones` from its
    /// key, pad 1 first: the §4 table, saturating past ±8. flx4-midi §4
    pub fn key_shift_pads(semitones: i8) -> [bool; 8] {
        // Bit 7 is pad 1. Index 0 is −9 or lower, 9 is no shift, 18 is +9 or higher.
        const BITMAP: [u8; 19] = [
            0b0000_1111, // < −8
            0b0000_1110, // −8
            0b0000_1101, // −7
            0b0000_1100, // −6
            0b0000_1010, // −5
            0b0000_1001, // −4
            0b0000_1000, // −3
            0b0000_0100, // −2
            0b0000_0010, // −1
            0b1000_0001, // 0
            0b0100_0000, // +1
            0b0010_0000, // +2
            0b0001_0000, // +3
            0b1001_0000, // +4
            0b0101_0000, // +5
            0b0011_0000, // +6
            0b1011_0000, // +7
            0b0111_0000, // +8
            0b1111_0000, // > +8
        ];
        let index = usize::try_from(semitones.clamp(-9, 9) + 9).unwrap_or(9);
        let bits = BITMAP.get(index).copied().unwrap_or(0);
        std::array::from_fn(|pad| bits & (0x80 >> pad) != 0)
    }

    fn write(&self, deck: Deck, out: &mut Vec<[u8; 3]>) {
        let channel = map::deck_channel(deck);
        let note = map::NOTE_ON | channel;
        // Each shift-layer address mirrors its LED, or it goes dark while
        // SHIFT is held. flx4-midi §3.1, §5.4
        let transport = [
            (map::PLAY, self.play),
            (map::SHIFT_PLAY_LED, self.play),
            (map::CUE, self.cue),
            (map::SHIFT_CUE_LED, self.cue),
            (map::SYNC, self.sync),
            (map::PFL, self.pfl),
            (map::LOOP_IN, self.loop_in),
            (map::SHIFT_LOOP_IN, self.loop_in),
            (map::LOOP_OUT, self.loop_out),
            (map::SHIFT_LOOP_OUT, self.loop_out),
            (map::RELOOP, self.reloop),
            (map::SHIFT_RELOOP, self.reloop),
        ];
        for (data1, on) in transport {
            out.push([note, data1, led(on)]);
        }
        // Mixxx only ever echoes 0x7F on the selected mode (flx4-midi §5.3);
        // the off writes for the other modes are ours [ASSUME harmless].
        for mode in PadMode::ALL {
            out.push([
                note,
                map::mode_button(mode).0,
                led(self.pad_mode == Some(mode)),
            ]);
        }
        out.push([map::CONTROL_CHANGE | channel, map::VU, self.level.min(0x7F)]);
        out.push([
            map::NOTE_ON | map::CH_JOG_RING,
            map::jog_ring(deck),
            led(self.loaded),
        ]);
        // Both pad layers, so the pads hold while SHIFT is down. flx4-midi §3.5
        let pads = [
            (PadMode::HotCue, &self.hot_cues),
            (PadMode::BeatLoop, &self.beat_loops),
            (PadMode::KeyShift, &self.key_shift),
        ];
        for pad_channel in map::pad_channels(deck) {
            for (mode, lit) in pads {
                let shown = self.pad_mode == Some(mode);
                let base = map::pad_base(mode);
                for (pad, &on) in (0..map::PAD_COUNT).zip(lit) {
                    out.push([map::NOTE_ON | pad_channel, base + pad, led(shown && on)]);
                }
            }
        }
    }
}

fn led(on: bool) -> u8 {
    if on {
        map::LED_ON
    } else {
        map::LED_OFF
    }
}
