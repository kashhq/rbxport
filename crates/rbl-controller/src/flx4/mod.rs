//! Pioneer DDJ-FLX4. flx4-midi = `docs/perform/flx4-midi.md`.

mod leds;
mod map;
#[cfg(test)]
mod tests;

use tracing::trace;

use crate::{
    Button, Controller, Deck, DeckControl, Event, FxChannel, GlobalButton, GlobalControl,
    JogSurface, PadMode,
};

pub use leds::{DeckLeds, Leds};

/// A key-shift pad's meaning, from [`Flx4::key_shift_pad`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyShiftPad {
    /// Semitones from the track's own key.
    Set(i8),
    /// Semitones from the deck's current shift.
    Step(i8),
}

/// The FLX4 decoder. Holds the latched MSB of every 14-bit control.
#[derive(Debug, Clone, Default)]
pub struct Flx4 {
    /// Last MSB per (channel, MSB CC). Zero until one arrives, so an LSB with
    /// no latched MSB decodes with MSB 0. Kept after the LSB: the next LSB-only
    /// move reuses it, as 14-bit MIDI intends. flx4-midi §5.6
    msb: [[u8; map::LSB_OFFSET as usize]; 16],
}

impl Flx4 {
    /// Send at startup (the controller answers with every knob and fader
    /// position) and then every [`Self::KEEPALIVE_INTERVAL_MS`]. flx4-midi §5.1
    pub const KEEPALIVE: [u8; 12] = map::KEEPALIVE;
    pub const KEEPALIVE_INTERVAL_MS: u64 = map::KEEPALIVE_INTERVAL_MS;

    pub fn new() -> Self {
        Self::default()
    }

    /// What a key-shift pad asks for. Unshifted pads set the key outright,
    /// 0, +1, +2, +3, −3, −2, −1, 0; shifted pads step it, +1, +2, +3, +4,
    /// −4, −3, −2, −1 (flx4-midi §4, Mixxx's meaning). `None` past pad 8.
    pub fn key_shift_pad(pad: u8, shift: bool) -> Option<KeyShiftPad> {
        const SET: [i8; 8] = [0, 1, 2, 3, -3, -2, -1, 0];
        const STEP: [i8; 8] = [1, 2, 3, 4, -4, -3, -2, -1];
        let table = if shift { &STEP } else { &SET };
        let semitones = *table.get(usize::from(pad))?;
        Some(if shift {
            KeyShiftPad::Step(semitones)
        } else {
            KeyShiftPad::Set(semitones)
        })
    }

    /// Decode one MIDI message (a 3-byte note or CC; `SysEx` and anything else
    /// returns `None`). A 14-bit MSB is latched and returns `None`.
    pub fn decode(&mut self, message: &[u8]) -> Option<Event> {
        let &[status, data1, data2] = message else {
            return unmapped(message);
        };
        // A status byte has the high bit set and data bytes do not.
        if status & 0x80 == 0 || (data1 | data2) & 0x80 != 0 {
            return unmapped(message);
        }
        let channel = status & 0x0F;
        let event = match status & 0xF0 {
            map::NOTE_ON => note(channel, data1, data2 != 0),
            map::NOTE_OFF => note(channel, data1, false),
            map::CONTROL_CHANGE => return self.control(channel, data1, data2, message),
            _ => None,
        };
        event.or_else(|| unmapped(message))
    }

    fn control(&mut self, channel: u8, cc: u8, value: u8, message: &[u8]) -> Option<Event> {
        let direct = match channel {
            map::CH_DECK_A => jog(Deck::A, cc, value),
            map::CH_DECK_B => jog(Deck::B, cc, value),
            map::CH_GLOBAL => browse(cc, value),
            map::CH_FX_1 if cc == map::FX_LEVEL => Some(Event::FxLevel { value }),
            _ => None,
        };
        if direct.is_some() {
            return direct;
        }
        // Standard 14-bit pairing: MSB in 0x00..0x20, its LSB at MSB + 0x20.
        let (msb, is_lsb) = match cc {
            0x00..map::LSB_OFFSET => (cc, false),
            map::LSB_OFFSET..0x40 => (cc - map::LSB_OFFSET, true),
            _ => return unmapped(message),
        };
        let Some(target) = Fourteen::of(channel, msb) else {
            return unmapped(message);
        };
        let latched = &mut self.msb[usize::from(channel)][usize::from(msb)];
        if !is_lsb {
            *latched = value;
            return None;
        }
        Some(target.event((u16::from(*latched) << 7) | u16::from(value)))
    }
}

impl Controller for Flx4 {
    fn decode(&mut self, message: &[u8]) -> Option<Event> {
        Flx4::decode(self, message)
    }
}

/// What a 14-bit pair drives.
#[derive(Clone, Copy)]
enum Fourteen {
    Deck(Deck, DeckControl),
    Global(GlobalControl),
}

impl Fourteen {
    /// flx4-midi §2.3, §2.5, §2.6
    fn of(channel: u8, msb: u8) -> Option<Self> {
        let deck = match channel {
            map::CH_DECK_A => Deck::A,
            map::CH_DECK_B => Deck::B,
            // Ch 7 carries the mixer-wide pairs; the filter knobs belong to a deck.
            map::CH_GLOBAL => {
                return match msb {
                    map::FILTER_A_MSB => Some(Self::Deck(Deck::A, DeckControl::Filter)),
                    map::FILTER_B_MSB => Some(Self::Deck(Deck::B, DeckControl::Filter)),
                    map::CROSSFADER_MSB => Some(Self::Global(GlobalControl::Crossfader)),
                    map::HEADPHONE_MIX_MSB => Some(Self::Global(GlobalControl::HeadphoneMix)),
                    _ => None,
                };
            }
            _ => return None,
        };
        let control = match msb {
            map::TEMPO_MSB => DeckControl::Tempo,
            map::TRIM_MSB => DeckControl::Trim,
            map::EQ_HIGH_MSB => DeckControl::EqHigh,
            map::EQ_MID_MSB => DeckControl::EqMid,
            map::EQ_LOW_MSB => DeckControl::EqLow,
            map::CHANNEL_FADER_MSB => DeckControl::ChannelFader,
            _ => return None,
        };
        Some(Self::Deck(deck, control))
    }

    fn event(self, value: u16) -> Event {
        match self {
            Self::Deck(deck, control) => Event::Deck14 {
                deck,
                control,
                value,
            },
            Self::Global(control) => Event::Global14 { control, value },
        }
    }
}

fn unmapped(message: &[u8]) -> Option<Event> {
    trace!(?message, "flx4: unmapped MIDI message");
    None
}

fn note(channel: u8, note: u8, pressed: bool) -> Option<Event> {
    match channel {
        map::CH_DECK_A => deck_note(Deck::A, note, pressed),
        map::CH_DECK_B => deck_note(Deck::B, note, pressed),
        map::CH_FX_1 => fx_note(FxChannel::One, note, pressed),
        map::CH_FX_2 => fx_note(FxChannel::Two, note, pressed),
        map::CH_GLOBAL => global_note(note, pressed),
        map::CH_PADS_A => pad(Deck::A, false, note, pressed),
        map::CH_PADS_A_SHIFT => pad(Deck::A, true, note, pressed),
        map::CH_PADS_B => pad(Deck::B, false, note, pressed),
        map::CH_PADS_B_SHIFT => pad(Deck::B, true, note, pressed),
        _ => None,
    }
}

/// flx4-midi §2.1, §2.2, §2.4, §4
fn deck_note(deck: Deck, note: u8, pressed: bool) -> Option<Event> {
    let (button, shift) = match note {
        map::SHIFT => (Button::Shift, false),
        map::PLAY => (Button::Play, false),
        map::SHIFT_PLAY => (Button::Play, true),
        map::CUE => (Button::Cue, false),
        map::SHIFT_CUE => (Button::Cue, true),
        map::SYNC => (Button::Sync, false),
        map::SYNC_LONG => (Button::SyncLong, false),
        map::SHIFT_SYNC => (Button::Sync, true),
        map::PFL => (Button::Pfl, false),
        map::SHIFT_PFL => (Button::Pfl, true),
        map::LOOP_IN => (Button::LoopIn, false),
        map::SHIFT_LOOP_IN => (Button::LoopIn, true),
        map::LOOP_OUT => (Button::LoopOut, false),
        map::SHIFT_LOOP_OUT => (Button::LoopOut, true),
        map::RELOOP => (Button::Reloop, false),
        map::SHIFT_RELOOP => (Button::Reloop, true),
        map::CUE_LOOP_CALL_BACK => (Button::CueLoopCallBack, false),
        map::SHIFT_CUE_LOOP_CALL_BACK => (Button::CueLoopCallBack, true),
        map::CUE_LOOP_CALL_FORWARD => (Button::CueLoopCallForward, false),
        map::SHIFT_CUE_LOOP_CALL_FORWARD => (Button::CueLoopCallForward, true),
        map::JOG_TOUCH => (Button::JogTouch, false),
        map::SHIFT_JOG_TOUCH => (Button::JogTouch, true),
        _ => PadMode::ALL.into_iter().find_map(|mode| {
            let (button_note, shift) = map::mode_button(mode);
            (button_note == note).then_some((Button::PadMode(mode), shift))
        })?,
    };
    Some(Event::Button {
        deck,
        button,
        shift,
        pressed,
    })
}

/// flx4-midi §2.8
fn fx_note(fx: FxChannel, note: u8, pressed: bool) -> Option<Event> {
    let global = |button, shift| {
        Some(Event::Global {
            button,
            shift,
            pressed,
        })
    };
    match (fx, note) {
        (FxChannel::One, map::FX_CH_SELECT_1) | (FxChannel::Two, map::FX_CH_SELECT_2) => {
            Some(Event::FxChannel {
                channel: fx,
                selected: pressed,
            })
        }
        (_, map::BEAT_FX_ON_OFF) => global(GlobalButton::BeatFxOnOff, false),
        (_, map::SHIFT_BEAT_FX_ON_OFF) => global(GlobalButton::BeatFxOnOff, true),
        (FxChannel::One, map::BEAT_FX_SELECT) => global(GlobalButton::BeatFxSelect, false),
        (FxChannel::One, map::SHIFT_BEAT_FX_SELECT) => global(GlobalButton::BeatFxSelect, true),
        (FxChannel::One, map::BEAT_FX_LEFT) => global(GlobalButton::BeatFxLeft, false),
        (FxChannel::One, map::BEAT_FX_RIGHT) => global(GlobalButton::BeatFxRight, false),
        _ => None,
    }
}

/// flx4-midi §2.7
fn global_note(note: u8, pressed: bool) -> Option<Event> {
    let (button, shift) = match note {
        map::BROWSE_PRESS => (GlobalButton::BrowsePress, false),
        map::SHIFT_BROWSE_PRESS => (GlobalButton::BrowsePress, true),
        map::LOAD_A => (GlobalButton::Load(Deck::A), false),
        map::LOAD_B => (GlobalButton::Load(Deck::B), false),
        _ => return None,
    };
    Some(Event::Global {
        button,
        shift,
        pressed,
    })
}

/// Pads: the mode is in the note's high nibble, the pad in the low three bits.
/// flx4-midi §4
fn pad(deck: Deck, shift: bool, note: u8, pressed: bool) -> Option<Event> {
    let offset = note & 0x0F;
    if offset >= map::PAD_COUNT {
        return None;
    }
    let base = note - offset;
    let mode = PadMode::ALL
        .into_iter()
        .find(|&mode| map::pad_base(mode) == base)?;
    Some(Event::Pad {
        deck,
        mode,
        pad: offset,
        shift,
        pressed,
    })
}

/// Jog rotation: offset binary around 0x40. flx4-midi §2.2, §5.9
fn jog(deck: Deck, cc: u8, value: u8) -> Option<Event> {
    let (surface, shift) = match cc {
        map::JOG_PLATTER_VINYL | map::JOG_PLATTER => (JogSurface::Platter, false),
        map::SHIFT_JOG_PLATTER => (JogSurface::Platter, true),
        map::JOG_SIDE => (JogSurface::Side, false),
        _ => return None,
    };
    // value is a 7-bit data byte, so it fits i8 and the subtraction cannot overflow.
    let delta = i8::try_from(value).ok()? - i8::try_from(map::JOG_CENTRE).ok()?;
    Some(Event::Jog {
        deck,
        surface,
        delta,
        shift,
    })
}

/// Browse rotation: 7-bit two's complement around 0x00. flx4-midi §2.7, §5.9
/// The shifted encoder uses the same decoding [ASSUME]: Mixxx only checks
/// 0x7F against everything else, which agrees for ±1 ticks.
fn browse(cc: u8, value: u8) -> Option<Event> {
    let shift = match cc {
        map::BROWSE => false,
        map::SHIFT_BROWSE => true,
        _ => return None,
    };
    let delta = i8::try_from(i16::from(value) - if value & 0x40 == 0 { 0 } else { 0x80 }).ok()?;
    Some(Event::Browse { delta, shift })
}
