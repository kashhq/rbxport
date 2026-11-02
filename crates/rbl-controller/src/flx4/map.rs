//! DDJ-FLX4 note and CC numbers, one constant per control.
//!
//! Source: `docs/perform/flx4-midi.md` (section cited on each line), itself
//! `[OBS mapping]` from Mixxx's FLX4 mapping. `[ASSUME]` marks inferences.
//! An LED is addressed with the same status and data1 as its button
//! (flx4-midi §3), so the input constants double as LED addresses.

use crate::{Deck, PadMode};

// Status kinds (high nibble); the low nibble is the 0-based channel.
pub const NOTE_OFF: u8 = 0x80; // never sent by the FLX4, accepted as a release, flx4-midi §1
pub const NOTE_ON: u8 = 0x90; // flx4-midi §1
pub const CONTROL_CHANGE: u8 = 0xB0; // flx4-midi §1

// 0-based MIDI channels. flx4-midi §1
pub const CH_DECK_A: u8 = 0; // Ch 1
pub const CH_DECK_B: u8 = 1; // Ch 2
pub const CH_FX_1: u8 = 4; // Ch 5, Beat FX with CH SELECT = CH1
pub const CH_FX_2: u8 = 5; // Ch 6, Beat FX with CH SELECT = CH2
pub const CH_GLOBAL: u8 = 6; // Ch 7
pub const CH_PADS_A: u8 = 7; // Ch 8
pub const CH_PADS_A_SHIFT: u8 = 8; // Ch 9
pub const CH_PADS_B: u8 = 9; // Ch 10
pub const CH_PADS_B_SHIFT: u8 = 10; // Ch 11
pub const CH_JOG_RING: u8 = 15; // Ch 16, output only, flx4-midi §3.3

// LED values: on/off only, no colour encoding. flx4-midi §3
pub const LED_ON: u8 = 0x7F;
pub const LED_OFF: u8 = 0x00;

// Deck transport notes. flx4-midi §2.1
pub const SHIFT: u8 = 0x3F;
pub const PLAY: u8 = 0x0B;
pub const SHIFT_PLAY: u8 = 0x0E;
pub const CUE: u8 = 0x0C;
pub const SHIFT_CUE: u8 = 0x48;
pub const SYNC: u8 = 0x58;
pub const SYNC_LONG: u8 = 0x5C;
pub const SHIFT_SYNC: u8 = 0x60;
pub const PFL: u8 = 0x54;
pub const SHIFT_PFL: u8 = 0x68;

// Jog wheel. flx4-midi §2.2
pub const JOG_TOUCH: u8 = 0x36; // note
pub const SHIFT_JOG_TOUCH: u8 = 0x67; // note
pub const JOG_PLATTER_VINYL: u8 = 0x22; // CC, vinyl mode on
pub const JOG_PLATTER: u8 = 0x23; // CC, vinyl mode off
pub const JOG_SIDE: u8 = 0x21; // CC
pub const SHIFT_JOG_PLATTER: u8 = 0x29; // CC
pub const JOG_CENTRE: u8 = 0x40; // relative offset-binary centre

// Loop and cue/loop call notes. flx4-midi §2.4
pub const LOOP_IN: u8 = 0x10;
pub const SHIFT_LOOP_IN: u8 = 0x4C;
pub const LOOP_OUT: u8 = 0x11;
pub const SHIFT_LOOP_OUT: u8 = 0x4E;
pub const RELOOP: u8 = 0x4D;
pub const SHIFT_RELOOP: u8 = 0x50;
pub const CUE_LOOP_CALL_BACK: u8 = 0x51;
pub const SHIFT_CUE_LOOP_CALL_BACK: u8 = 0x3E;
pub const CUE_LOOP_CALL_FORWARD: u8 = 0x53;
pub const SHIFT_CUE_LOOP_CALL_FORWARD: u8 = 0x3D;

// Pad-mode buttons on the deck channel. flx4-midi §4
pub const MODE_HOT_CUE: u8 = 0x1B;
pub const MODE_KEYBOARD: u8 = 0x69; // SHIFT + HOT CUE
pub const MODE_PAD_FX1: u8 = 0x1E;
pub const MODE_PAD_FX2: u8 = 0x6B; // SHIFT + PAD FX1
pub const MODE_BEAT_JUMP: u8 = 0x20;
pub const MODE_BEAT_LOOP: u8 = 0x6D; // SHIFT + BEAT JUMP
pub const MODE_SAMPLER: u8 = 0x22;
pub const MODE_KEY_SHIFT: u8 = 0x6F; // SHIFT + SAMPLER

// Pad note bases; pad N (0..=7) is base + N. flx4-midi §4
pub const PAD_COUNT: u8 = 8;
pub const PAD_BASE_HOT_CUE: u8 = 0x00;
pub const PAD_BASE_PAD_FX1: u8 = 0x10; // [ASSUME] inferred from the 0x10 spacing, unmapped
pub const PAD_BASE_BEAT_JUMP: u8 = 0x20;
pub const PAD_BASE_SAMPLER: u8 = 0x30;
pub const PAD_BASE_KEYBOARD: u8 = 0x40;
pub const PAD_BASE_PAD_FX2: u8 = 0x50; // [ASSUME] inferred from the 0x10 spacing, unmapped
pub const PAD_BASE_BEAT_LOOP: u8 = 0x60;
pub const PAD_BASE_KEY_SHIFT: u8 = 0x70;

// 14-bit pairs: MSB first, LSB = MSB + 0x20. flx4-midi §2.5, §5.6
pub const LSB_OFFSET: u8 = 0x20;
pub const TEMPO_MSB: u8 = 0x00; // deck channel, flx4-midi §2.3
pub const TRIM_MSB: u8 = 0x04; // deck channel, flx4-midi §2.5
pub const EQ_HIGH_MSB: u8 = 0x07; // deck channel, flx4-midi §2.5
pub const EQ_MID_MSB: u8 = 0x0B; // deck channel, flx4-midi §2.5
pub const EQ_LOW_MSB: u8 = 0x0F; // deck channel, flx4-midi §2.5
pub const CHANNEL_FADER_MSB: u8 = 0x13; // deck channel, flx4-midi §2.5
pub const FILTER_A_MSB: u8 = 0x17; // Ch 7, flx4-midi §2.5
pub const FILTER_B_MSB: u8 = 0x18; // Ch 7, flx4-midi §2.5
pub const CROSSFADER_MSB: u8 = 0x1F; // Ch 7, flx4-midi §2.6
pub const HEADPHONE_MIX_MSB: u8 = 0x0C; // Ch 7, flx4-midi §2.6

// Browse and load on Ch 7. flx4-midi §2.7
pub const BROWSE: u8 = 0x40; // CC, two's complement
pub const SHIFT_BROWSE: u8 = 0x64; // CC
pub const BROWSE_PRESS: u8 = 0x41; // note
pub const SHIFT_BROWSE_PRESS: u8 = 0x42; // note
pub const LOAD_A: u8 = 0x46; // note
pub const LOAD_B: u8 = 0x47; // note

// Beat FX on Ch 5 / Ch 6. flx4-midi §2.8, §3.4
pub const BEAT_FX_SELECT: u8 = 0x63; // Ch 5
pub const SHIFT_BEAT_FX_SELECT: u8 = 0x64; // Ch 5
pub const BEAT_FX_LEFT: u8 = 0x4A; // Ch 5
pub const BEAT_FX_RIGHT: u8 = 0x4B; // Ch 5
pub const FX_CH_SELECT_1: u8 = 0x10; // Ch 5
pub const FX_CH_SELECT_2: u8 = 0x11; // Ch 6
pub const BEAT_FX_ON_OFF: u8 = 0x47; // Ch 5 or Ch 6; the LED is driven on Ch 5 only (§3.4)
pub const SHIFT_BEAT_FX_ON_OFF: u8 = 0x43; // Ch 5 or Ch 6
pub const FX_LEVEL: u8 = 0x02; // CC on Ch 5, used as 7-bit

// LED-only addresses.
pub const SHIFT_PLAY_LED: u8 = 0x47; // flx4-midi §3.1, §5.4
pub const SHIFT_CUE_LED: u8 = SHIFT_CUE; // 0x48, not the XML's 0x47, flx4-midi §5.10
pub const VU: u8 = 0x02; // CC on the deck channel, deck 2 is 0xB1 (fixes §5.10), flx4-midi §3.2
pub const JOG_RING_A: u8 = 0x00; // flx4-midi §3.3
pub const JOG_RING_B: u8 = 0x01; // flx4-midi §3.3

/// The `SysEx` the host sends at startup and every 200 ms. flx4-midi §5.1
pub const KEEPALIVE: [u8; 12] = [
    0xF0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x05, 0x00, 0x50, 0x02, 0xF7,
];
pub const KEEPALIVE_INTERVAL_MS: u64 = 200;

pub const fn deck_channel(deck: Deck) -> u8 {
    match deck {
        Deck::A => CH_DECK_A,
        Deck::B => CH_DECK_B,
    }
}

/// The unshifted then the shifted pad channel. flx4-midi §1
pub const fn pad_channels(deck: Deck) -> [u8; 2] {
    match deck {
        Deck::A => [CH_PADS_A, CH_PADS_A_SHIFT],
        Deck::B => [CH_PADS_B, CH_PADS_B_SHIFT],
    }
}

pub const fn jog_ring(deck: Deck) -> u8 {
    match deck {
        Deck::A => JOG_RING_A,
        Deck::B => JOG_RING_B,
    }
}

/// The mode button's note, and whether it is the SHIFT-selected secondary
/// mode. flx4-midi §4
pub const fn mode_button(mode: PadMode) -> (u8, bool) {
    match mode {
        PadMode::HotCue => (MODE_HOT_CUE, false),
        PadMode::Keyboard => (MODE_KEYBOARD, true),
        PadMode::PadFx1 => (MODE_PAD_FX1, false),
        PadMode::PadFx2 => (MODE_PAD_FX2, true),
        PadMode::BeatJump => (MODE_BEAT_JUMP, false),
        PadMode::BeatLoop => (MODE_BEAT_LOOP, true),
        PadMode::Sampler => (MODE_SAMPLER, false),
        PadMode::KeyShift => (MODE_KEY_SHIFT, true),
    }
}

/// flx4-midi §4
pub const fn pad_base(mode: PadMode) -> u8 {
    match mode {
        PadMode::HotCue => PAD_BASE_HOT_CUE,
        PadMode::Keyboard => PAD_BASE_KEYBOARD,
        PadMode::PadFx1 => PAD_BASE_PAD_FX1,
        PadMode::PadFx2 => PAD_BASE_PAD_FX2,
        PadMode::BeatJump => PAD_BASE_BEAT_JUMP,
        PadMode::BeatLoop => PAD_BASE_BEAT_LOOP,
        PadMode::Sampler => PAD_BASE_SAMPLER,
        PadMode::KeyShift => PAD_BASE_KEY_SHIFT,
    }
}
