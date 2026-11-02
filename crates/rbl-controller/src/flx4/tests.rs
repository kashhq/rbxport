//! Byte-level tests against `docs/perform/flx4-midi.md`. Bytes are written
//! out literally, not taken from `map`, so a wrong constant fails here.

use std::collections::HashSet;

use crate::{
    Button, Controller, Deck, DeckControl, DeckLeds, Event, Flx4, FxChannel, GlobalButton,
    GlobalControl, JogSurface, KeyShiftPad, Leds, PadMode,
};

const fn button(deck: Deck, button: Button, shift: bool) -> Event {
    Event::Button {
        deck,
        button,
        shift,
        pressed: true,
    }
}

const fn global(button: GlobalButton, shift: bool) -> Event {
    Event::Global {
        button,
        shift,
        pressed: true,
    }
}

const fn pad(deck: Deck, mode: PadMode, pad: u8, shift: bool) -> Event {
    Event::Pad {
        deck,
        mode,
        pad,
        shift,
        pressed: true,
    }
}

const fn jog(deck: Deck, surface: JogSurface, delta: i8, shift: bool) -> Event {
    Event::Jog {
        deck,
        surface,
        delta,
        shift,
    }
}

/// Every single-message input row in flx4-midi §2 and §4.
const SINGLE: &[([u8; 3], Event)] = &[
    // §2.1 transport, deck 1
    ([0x90, 0x3F, 0x7F], button(Deck::A, Button::Shift, false)),
    ([0x90, 0x0B, 0x7F], button(Deck::A, Button::Play, false)),
    ([0x90, 0x0E, 0x7F], button(Deck::A, Button::Play, true)),
    ([0x90, 0x0C, 0x7F], button(Deck::A, Button::Cue, false)),
    ([0x90, 0x48, 0x7F], button(Deck::A, Button::Cue, true)),
    ([0x90, 0x58, 0x7F], button(Deck::A, Button::Sync, false)),
    ([0x90, 0x5C, 0x7F], button(Deck::A, Button::SyncLong, false)),
    ([0x90, 0x60, 0x7F], button(Deck::A, Button::Sync, true)),
    ([0x90, 0x54, 0x7F], button(Deck::A, Button::Pfl, false)),
    ([0x90, 0x68, 0x7F], button(Deck::A, Button::Pfl, true)),
    // §2.1 deck 2 uses identical data1 on Ch 2
    ([0x91, 0x3F, 0x7F], button(Deck::B, Button::Shift, false)),
    ([0x91, 0x0B, 0x7F], button(Deck::B, Button::Play, false)),
    ([0x91, 0x5C, 0x7F], button(Deck::B, Button::SyncLong, false)),
    // §2.2 jog: offset binary around 0x40
    (
        [0xB0, 0x22, 0x41],
        jog(Deck::A, JogSurface::Platter, 1, false),
    ),
    (
        [0xB0, 0x23, 0x3F],
        jog(Deck::A, JogSurface::Platter, -1, false),
    ),
    ([0xB0, 0x21, 0x45], jog(Deck::A, JogSurface::Side, 5, false)),
    (
        [0xB0, 0x29, 0x30],
        jog(Deck::A, JogSurface::Platter, -16, true),
    ),
    (
        [0xB1, 0x22, 0x00],
        jog(Deck::B, JogSurface::Platter, -64, false),
    ),
    (
        [0xB1, 0x23, 0x7F],
        jog(Deck::B, JogSurface::Platter, 63, false),
    ),
    ([0xB1, 0x21, 0x40], jog(Deck::B, JogSurface::Side, 0, false)),
    ([0x90, 0x36, 0x7F], button(Deck::A, Button::JogTouch, false)),
    ([0x91, 0x67, 0x7F], button(Deck::B, Button::JogTouch, true)),
    // §2.4 loop and cue/loop call
    ([0x90, 0x10, 0x7F], button(Deck::A, Button::LoopIn, false)),
    ([0x90, 0x4C, 0x7F], button(Deck::A, Button::LoopIn, true)),
    ([0x90, 0x11, 0x7F], button(Deck::A, Button::LoopOut, false)),
    ([0x90, 0x4E, 0x7F], button(Deck::A, Button::LoopOut, true)),
    ([0x90, 0x4D, 0x7F], button(Deck::A, Button::Reloop, false)),
    ([0x90, 0x50, 0x7F], button(Deck::A, Button::Reloop, true)),
    (
        [0x91, 0x51, 0x7F],
        button(Deck::B, Button::CueLoopCallBack, false),
    ),
    (
        [0x91, 0x3E, 0x7F],
        button(Deck::B, Button::CueLoopCallBack, true),
    ),
    (
        [0x91, 0x53, 0x7F],
        button(Deck::B, Button::CueLoopCallForward, false),
    ),
    (
        [0x91, 0x3D, 0x7F],
        button(Deck::B, Button::CueLoopCallForward, true),
    ),
    // §2.7 browse: two's complement around 0x00
    (
        [0xB6, 0x40, 0x01],
        Event::Browse {
            delta: 1,
            shift: false,
        },
    ),
    (
        [0xB6, 0x40, 0x7F],
        Event::Browse {
            delta: -1,
            shift: false,
        },
    ),
    (
        [0xB6, 0x40, 0x03],
        Event::Browse {
            delta: 3,
            shift: false,
        },
    ),
    (
        [0xB6, 0x40, 0x7D],
        Event::Browse {
            delta: -3,
            shift: false,
        },
    ),
    (
        [0xB6, 0x64, 0x7F],
        Event::Browse {
            delta: -1,
            shift: true,
        },
    ),
    (
        [0xB6, 0x64, 0x01],
        Event::Browse {
            delta: 1,
            shift: true,
        },
    ),
    ([0x96, 0x41, 0x7F], global(GlobalButton::BrowsePress, false)),
    ([0x96, 0x42, 0x7F], global(GlobalButton::BrowsePress, true)),
    (
        [0x96, 0x46, 0x7F],
        global(GlobalButton::Load(Deck::A), false),
    ),
    (
        [0x96, 0x47, 0x7F],
        global(GlobalButton::Load(Deck::B), false),
    ),
    // §2.8 beat FX
    (
        [0x94, 0x63, 0x7F],
        global(GlobalButton::BeatFxSelect, false),
    ),
    ([0x94, 0x64, 0x7F], global(GlobalButton::BeatFxSelect, true)),
    ([0x94, 0x4A, 0x7F], global(GlobalButton::BeatFxLeft, false)),
    ([0x94, 0x4B, 0x7F], global(GlobalButton::BeatFxRight, false)),
    (
        [0x94, 0x10, 0x7F],
        Event::FxChannel {
            channel: FxChannel::One,
            selected: true,
        },
    ),
    (
        [0x94, 0x10, 0x00],
        Event::FxChannel {
            channel: FxChannel::One,
            selected: false,
        },
    ),
    (
        [0x95, 0x11, 0x7F],
        Event::FxChannel {
            channel: FxChannel::Two,
            selected: true,
        },
    ),
    ([0x94, 0x47, 0x7F], global(GlobalButton::BeatFxOnOff, false)),
    ([0x95, 0x47, 0x7F], global(GlobalButton::BeatFxOnOff, false)),
    ([0x94, 0x43, 0x7F], global(GlobalButton::BeatFxOnOff, true)),
    ([0x95, 0x43, 0x7F], global(GlobalButton::BeatFxOnOff, true)),
    ([0xB4, 0x02, 0x55], Event::FxLevel { value: 0x55 }),
    // §4 pad-mode buttons; secondary modes are the shifted variant
    (
        [0x90, 0x1B, 0x7F],
        button(Deck::A, Button::PadMode(PadMode::HotCue), false),
    ),
    (
        [0x90, 0x69, 0x7F],
        button(Deck::A, Button::PadMode(PadMode::Keyboard), true),
    ),
    (
        [0x90, 0x1E, 0x7F],
        button(Deck::A, Button::PadMode(PadMode::PadFx1), false),
    ),
    (
        [0x90, 0x6B, 0x7F],
        button(Deck::A, Button::PadMode(PadMode::PadFx2), true),
    ),
    (
        [0x91, 0x20, 0x7F],
        button(Deck::B, Button::PadMode(PadMode::BeatJump), false),
    ),
    (
        [0x91, 0x6D, 0x7F],
        button(Deck::B, Button::PadMode(PadMode::BeatLoop), true),
    ),
    (
        [0x91, 0x22, 0x7F],
        button(Deck::B, Button::PadMode(PadMode::Sampler), false),
    ),
    (
        [0x91, 0x6F, 0x7F],
        button(Deck::B, Button::PadMode(PadMode::KeyShift), true),
    ),
    // §4 pads: unshifted 0x97/0x99, shifted on the next channel with the same note
    ([0x97, 0x00, 0x7F], pad(Deck::A, PadMode::HotCue, 0, false)),
    ([0x98, 0x07, 0x7F], pad(Deck::A, PadMode::HotCue, 7, true)),
    ([0x97, 0x13, 0x7F], pad(Deck::A, PadMode::PadFx1, 3, false)),
    (
        [0x97, 0x21, 0x7F],
        pad(Deck::A, PadMode::BeatJump, 1, false),
    ),
    ([0x98, 0x26, 0x7F], pad(Deck::A, PadMode::BeatJump, 6, true)),
    ([0x97, 0x34, 0x7F], pad(Deck::A, PadMode::Sampler, 4, false)),
    (
        [0x99, 0x45, 0x7F],
        pad(Deck::B, PadMode::Keyboard, 5, false),
    ),
    ([0x99, 0x52, 0x7F], pad(Deck::B, PadMode::PadFx2, 2, false)),
    ([0x9A, 0x60, 0x7F], pad(Deck::B, PadMode::BeatLoop, 0, true)),
    ([0x9A, 0x77, 0x7F], pad(Deck::B, PadMode::KeyShift, 7, true)),
];

/// 14-bit rows: (MSB message, LSB message, event). flx4-midi §2.3, §2.5, §2.6
const FOURTEEN: &[([u8; 3], [u8; 3], Event)] = &[
    (
        [0xB0, 0x00, 0x40],
        [0xB0, 0x20, 0x00],
        deck14(Deck::A, DeckControl::Tempo, 0x2000),
    ),
    (
        [0xB1, 0x00, 0x7F],
        [0xB1, 0x20, 0x7F],
        deck14(Deck::B, DeckControl::Tempo, 0x3FFF),
    ),
    (
        [0xB0, 0x04, 0x01],
        [0xB0, 0x24, 0x02],
        deck14(Deck::A, DeckControl::Trim, 0x0082),
    ),
    (
        [0xB0, 0x07, 0x10],
        [0xB0, 0x27, 0x00],
        deck14(Deck::A, DeckControl::EqHigh, 0x0800),
    ),
    (
        [0xB1, 0x0B, 0x20],
        [0xB1, 0x2B, 0x05],
        deck14(Deck::B, DeckControl::EqMid, 0x1005),
    ),
    (
        [0xB0, 0x0F, 0x00],
        [0xB0, 0x2F, 0x00],
        deck14(Deck::A, DeckControl::EqLow, 0x0000),
    ),
    (
        [0xB1, 0x13, 0x7F],
        [0xB1, 0x33, 0x00],
        deck14(Deck::B, DeckControl::ChannelFader, 0x3F80),
    ),
    (
        [0xB6, 0x17, 0x40],
        [0xB6, 0x37, 0x00],
        deck14(Deck::A, DeckControl::Filter, 0x2000),
    ),
    (
        [0xB6, 0x18, 0x41],
        [0xB6, 0x38, 0x01],
        deck14(Deck::B, DeckControl::Filter, 0x2081),
    ),
    (
        [0xB6, 0x1F, 0x00],
        [0xB6, 0x3F, 0x7F],
        global14(GlobalControl::Crossfader, 0x007F),
    ),
    (
        [0xB6, 0x0C, 0x55],
        [0xB6, 0x2C, 0x2A],
        global14(GlobalControl::HeadphoneMix, 0x2AAA),
    ),
];

const fn deck14(deck: Deck, control: DeckControl, value: u16) -> Event {
    Event::Deck14 {
        deck,
        control,
        value,
    }
}

const fn global14(control: GlobalControl, value: u16) -> Event {
    Event::Global14 { control, value }
}

#[test]
fn every_single_message_row_decodes() {
    for (bytes, expected) in SINGLE {
        assert_eq!(Flx4::new().decode(bytes), Some(*expected), "{bytes:02X?}");
    }
}

#[test]
fn velocity_zero_and_note_off_are_releases() {
    let mut flx4 = Flx4::new();
    let release = Some(Event::Button {
        deck: Deck::A,
        button: Button::Play,
        shift: false,
        pressed: false,
    });
    assert_eq!(flx4.decode(&[0x90, 0x0B, 0x00]), release);
    assert_eq!(flx4.decode(&[0x80, 0x0B, 0x40]), release);
    assert_eq!(
        flx4.decode(&[0x9A, 0x03, 0x00]),
        Some(Event::Pad {
            deck: Deck::B,
            mode: PadMode::HotCue,
            pad: 3,
            shift: true,
            pressed: false
        })
    );
}

#[test]
fn every_fourteen_bit_row_emits_on_lsb_only() {
    for (msb, lsb, expected) in FOURTEEN {
        let mut flx4 = Flx4::new();
        assert_eq!(flx4.decode(msb), None, "MSB alone {msb:02X?}");
        assert_eq!(flx4.decode(lsb), Some(*expected), "{msb:02X?} {lsb:02X?}");
    }
}

#[test]
fn lsb_without_msb_uses_zero_and_later_lsbs_reuse_the_latch() {
    let mut flx4 = Flx4::new();
    assert_eq!(
        flx4.decode(&[0xB0, 0x20, 0x05]),
        Some(deck14(Deck::A, DeckControl::Tempo, 0x0005))
    );
    assert_eq!(flx4.decode(&[0xB0, 0x00, 0x12]), None);
    assert_eq!(
        flx4.decode(&[0xB0, 0x20, 0x01]),
        Some(deck14(Deck::A, DeckControl::Tempo, 0x0901))
    );
    assert_eq!(
        flx4.decode(&[0xB0, 0x20, 0x02]),
        Some(deck14(Deck::A, DeckControl::Tempo, 0x0902))
    );
}

#[test]
fn latches_are_per_channel_and_per_control() {
    let mut flx4 = Flx4::new();
    assert_eq!(flx4.decode(&[0xB0, 0x00, 0x10]), None); // tempo A
    assert_eq!(flx4.decode(&[0xB0, 0x13, 0x7F]), None); // fader A
    assert_eq!(flx4.decode(&[0xB1, 0x00, 0x20]), None); // tempo B
    assert_eq!(
        flx4.decode(&[0xB0, 0x20, 0x00]),
        Some(deck14(Deck::A, DeckControl::Tempo, 0x0800))
    );
    assert_eq!(
        flx4.decode(&[0xB1, 0x20, 0x00]),
        Some(deck14(Deck::B, DeckControl::Tempo, 0x1000))
    );
    assert_eq!(
        flx4.decode(&[0xB0, 0x33, 0x7F]),
        Some(deck14(Deck::A, DeckControl::ChannelFader, 0x3FFF))
    );
}

#[test]
fn unknown_and_malformed_messages_decode_to_none() {
    let rejected: &[&[u8]] = &[
        &[],
        &[0x90, 0x0B],
        &[0x90, 0x0B, 0x7F, 0x00],
        &Flx4::KEEPALIVE,
        &[0x0B, 0x7F, 0x00], // no status byte
        &[0x90, 0x8B, 0x7F], // data1 high bit
        &[0x90, 0x0B, 0x80], // data2 high bit
        &[0x97, 0x08, 0x7F], // pad offset beyond 8
        &[0x9F, 0x00, 0x7F], // jog ring is output only
        &[0xB0, 0x02, 0x40], // VU is output only
        &[0xB4, 0x22, 0x40], // FX LEVEL LSB, unmapped
        &[0x95, 0x63, 0x7F], // FX select is Ch 5 only
        &[0x95, 0x10, 0x7F], // CH SELECT = CH1 is Ch 5 only
        &[0x96, 0x48, 0x7F],
        &[0xB6, 0x00, 0x40], // no tempo on Ch 7
        &[0xA0, 0x0B, 0x7F], // poly aftertouch
        &[0x92, 0x0B, 0x7F], // Ch 3 is unused
    ];
    for message in rejected {
        assert_eq!(Flx4::new().decode(message), None, "{message:02X?}");
    }
}

#[test]
fn controller_trait_decodes_through_a_trait_object() {
    let mut controller: Box<dyn Controller> = Box::new(Flx4::new());
    assert_eq!(
        controller.decode(&[0x91, 0x0B, 0x7F]),
        Some(button(Deck::B, Button::Play, false))
    );
}

#[test]
fn keepalive_bytes() {
    assert_eq!(
        Flx4::KEEPALIVE,
        [0xF0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x05, 0x00, 0x50, 0x02, 0xF7]
    );
    assert_eq!(Flx4::KEEPALIVE_INTERVAL_MS, 200);
}

const NOTHING: [[u8; 3]; 0] = [];

/// Messages from the default state to `leds`.
fn changes(leds: &Leds) -> Vec<[u8; 3]> {
    leds.messages(Some(&Leds::default()))
}

#[test]
fn startup_writes_every_address_once() {
    let messages = Leds::default().messages(None);
    assert_eq!(messages.len(), 142);
    let addresses: HashSet<[u8; 2]> = messages.iter().map(|&[s, d, _]| [s, d]).collect();
    assert_eq!(addresses.len(), messages.len(), "duplicate LED address");
    assert!(messages.iter().all(|&[_, d1, d2]| d1 < 0x80 && d2 == 0x00));
}

#[test]
fn unchanged_state_emits_nothing() {
    let mut leds = Leds::default();
    leds.decks[0].play = true;
    leds.decks[1].pad_mode = Some(PadMode::HotCue);
    leds.decks[1].hot_cues = [true; 8];
    assert_eq!(leds.messages(Some(&leds)), NOTHING);
}

#[test]
fn shift_layer_leds_mirror() {
    let mut leds = Leds::default();
    leds.decks[0].play = true;
    assert_eq!(changes(&leds), [[0x90, 0x0B, 0x7F], [0x90, 0x47, 0x7F]]);

    let mut leds = Leds::default();
    leds.decks[1].cue = true;
    assert_eq!(changes(&leds), [[0x91, 0x0C, 0x7F], [0x91, 0x48, 0x7F]]);

    let mut leds = Leds::default();
    leds.decks[0].loop_in = true;
    leds.decks[0].loop_out = true;
    leds.decks[0].reloop = true;
    assert_eq!(
        changes(&leds),
        [
            [0x90, 0x10, 0x7F],
            [0x90, 0x4C, 0x7F],
            [0x90, 0x11, 0x7F],
            [0x90, 0x4E, 0x7F],
            [0x90, 0x4D, 0x7F],
            [0x90, 0x50, 0x7F]
        ]
    );

    let leds = Leds {
        beat_fx: true,
        ..Leds::default()
    };
    assert_eq!(changes(&leds), [[0x94, 0x47, 0x7F], [0x94, 0x43, 0x7F]]);
}

#[test]
fn single_layer_leds() {
    let mut leds = Leds::default();
    leds.decks[1].sync = true;
    leds.decks[1].pfl = true;
    assert_eq!(changes(&leds), [[0x91, 0x58, 0x7F], [0x91, 0x54, 0x7F]]);
}

#[test]
fn vu_uses_each_decks_own_channel() {
    let mut leds = Leds::default();
    leds.decks[1].level = 0x40;
    assert_eq!(changes(&leds), [[0xB1, 0x02, 0x40]]);
    leds.decks[0].level = 0xFF;
    leds.decks[1].level = 0;
    assert_eq!(changes(&leds), [[0xB0, 0x02, 0x7F]]);
}

#[test]
fn jog_ring_follows_loaded() {
    let mut leds = Leds::default();
    leds.decks[1].loaded = true;
    assert_eq!(changes(&leds), [[0x9F, 0x01, 0x7F]]);
    leds.decks[0].loaded = true;
    assert_eq!(changes(&leds), [[0x9F, 0x00, 0x7F], [0x9F, 0x01, 0x7F]]);
}

#[test]
fn pad_leds_follow_the_shown_mode_on_both_layers() {
    let hot_cue = DeckLeds {
        pad_mode: Some(PadMode::HotCue),
        hot_cues: [false, false, true, false, false, false, false, false],
        beat_loops: [false, true, false, false, false, false, false, false],
        ..DeckLeds::default()
    };
    let leds = Leds {
        decks: [hot_cue, DeckLeds::default()],
        beat_fx: false,
    };
    assert_eq!(
        changes(&leds),
        [[0x90, 0x1B, 0x7F], [0x97, 0x02, 0x7F], [0x98, 0x02, 0x7F]]
    );

    // Switching mode clears the old mode's pads and lights the new one's.
    let mut beat_loop = leds;
    beat_loop.decks[0].pad_mode = Some(PadMode::BeatLoop);
    assert_eq!(
        beat_loop.messages(Some(&leds)),
        [
            [0x90, 0x1B, 0x00],
            [0x90, 0x6D, 0x7F],
            [0x97, 0x02, 0x00],
            [0x97, 0x61, 0x7F],
            [0x98, 0x02, 0x00],
            [0x98, 0x61, 0x7F],
        ]
    );

    // A hot cue changing while another mode is shown writes nothing.
    let mut hidden = beat_loop;
    hidden.decks[0].hot_cues[5] = true;
    assert_eq!(hidden.messages(Some(&beat_loop)), NOTHING);
}

#[test]
fn key_shift_bitmap_on_deck_two_pad_channels() {
    let key_shift = DeckLeds {
        pad_mode: Some(PadMode::KeyShift),
        // §4: +1 semitone is 0100 0000, pad 2.
        key_shift: [false, true, false, false, false, false, false, false],
        ..DeckLeds::default()
    };
    let leds = Leds {
        decks: [DeckLeds::default(), key_shift],
        beat_fx: false,
    };
    assert_eq!(
        changes(&leds),
        [[0x91, 0x6F, 0x7F], [0x99, 0x71, 0x7F], [0x9A, 0x71, 0x7F]]
    );
}

#[test]
fn blackout_clears_everything_and_the_sampler_pads() {
    let blackout = Leds::blackout();
    assert_eq!(blackout.len(), 142 + 4 * 8);
    assert!(blackout.iter().all(|&[_, _, value]| value == 0x00));
    for channel in [0x97, 0x98, 0x99, 0x9A] {
        for note in (0x00..=0x07).chain(0x30..=0x37) {
            assert!(
                blackout.contains(&[channel, note, 0x00]),
                "{channel:02X} {note:02X}"
            );
        }
    }
    assert!(blackout.contains(&[0xB1, 0x02, 0x00]));
    assert!(blackout.contains(&[0x9F, 0x01, 0x00]));
}

#[test]
fn key_shift_pads_follow_the_section_4_table() {
    let set: Vec<_> = (0..8).map(|pad| Flx4::key_shift_pad(pad, false)).collect();
    let expected: Vec<_> = [0, 1, 2, 3, -3, -2, -1, 0]
        .into_iter()
        .map(|s| Some(KeyShiftPad::Set(s)))
        .collect();
    assert_eq!(set, expected);
    let step: Vec<_> = (0..8).map(|pad| Flx4::key_shift_pad(pad, true)).collect();
    let expected: Vec<_> = [1, 2, 3, 4, -4, -3, -2, -1]
        .into_iter()
        .map(|s| Some(KeyShiftPad::Step(s)))
        .collect();
    assert_eq!(step, expected);
    assert_eq!(Flx4::key_shift_pad(8, false), None);
}

#[test]
fn key_shift_led_bitmap_matches_the_section_4_table() {
    // Written as the doc's bit strings, pad 1 first.
    let bits = |s: &str| -> [bool; 8] {
        let mut out = [false; 8];
        for (slot, c) in out.iter_mut().zip(s.chars().filter(|c| *c != ' ')) {
            *slot = c == '1';
        }
        out
    };
    let table = [
        (0, "1000 0001"),
        (1, "0100 0000"),
        (2, "0010 0000"),
        (3, "0001 0000"),
        (4, "1001 0000"),
        (5, "0101 0000"),
        (6, "0011 0000"),
        (7, "1011 0000"),
        (8, "0111 0000"),
        (9, "1111 0000"),
        (12, "1111 0000"),
        (-1, "0000 0010"),
        (-2, "0000 0100"),
        (-3, "0000 1000"),
        (-4, "0000 1001"),
        (-5, "0000 1010"),
        (-6, "0000 1100"),
        (-7, "0000 1101"),
        (-8, "0000 1110"),
        (-9, "0000 1111"),
        (i8::MIN, "0000 1111"),
    ];
    for (semitones, expected) in table {
        assert_eq!(
            DeckLeds::key_shift_pads(semitones),
            bits(expected),
            "{semitones:+} semitones"
        );
    }
}
