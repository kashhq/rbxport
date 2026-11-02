//! DJ-controller MIDI, as typed events and LED messages.
//!
//! A controller map turns one raw MIDI message into an [`Event`] and an LED
//! model into the MIDI messages that light the hardware. Nothing here opens a
//! device or knows about Tauri: `src-tauri/src/perform` owns the `midir`
//! connection, feeds each incoming message to [`Controller::decode`], and
//! writes what [`Leds::messages`] returns.
//!
//! # Evidence
//!
//! The DDJ-FLX4 map follows `docs/perform/flx4-midi.md`, which was extracted
//! from Mixxx's community mapping. Unless a line says otherwise it is
//! `[OBS mapping]`: what that mapping uses, not a capture of the physical
//! controller.

mod flx4;

pub use flx4::{DeckLeds, Flx4, Leds};

/// A player deck. On the FLX4 deck 1 is `A` (MIDI channel 1) and deck 2 is `B`
/// (channel 2). flx4-midi §1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Deck {
    A,
    B,
}

impl Deck {
    pub const ALL: [Deck; 2] = [Deck::A, Deck::B];
}

/// The eight pad modes. The controller switches mode itself and the pads then
/// send that mode's note range, so a pad event already carries its mode.
/// flx4-midi §4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadMode {
    HotCue,
    Keyboard,
    PadFx1,
    PadFx2,
    BeatJump,
    BeatLoop,
    Sampler,
    KeyShift,
}

impl PadMode {
    pub const ALL: [PadMode; 8] = [
        PadMode::HotCue,
        PadMode::Keyboard,
        PadMode::PadFx1,
        PadMode::PadFx2,
        PadMode::BeatJump,
        PadMode::BeatLoop,
        PadMode::Sampler,
        PadMode::KeyShift,
    ];
}

/// Deck-scoped buttons. flx4-midi §2.1, §2.2, §2.4, §4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    /// The SHIFT key itself, so the host can track it for controls the
    /// hardware does not shift (Beat FX LEVEL/DEPTH, flx4-midi §5.5).
    Shift,
    Play,
    Cue,
    /// BEAT SYNC short press.
    Sync,
    /// BEAT SYNC long press, timed by the controller (flx4-midi §5.7).
    SyncLong,
    /// Channel CUE (headphone cue).
    Pfl,
    LoopIn,
    LoopOut,
    Reloop,
    CueLoopCallBack,
    CueLoopCallForward,
    /// A pad-mode button. Secondary modes arrive with `shift: true`.
    PadMode(PadMode),
    /// Hand on the platter: the only reliable touch signal (flx4-midi §5.8).
    JogTouch,
}

/// Buttons that belong to no deck. flx4-midi §2.7, §2.8
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalButton {
    BrowsePress,
    Load(Deck),
    BeatFxSelect,
    BeatFxLeft,
    BeatFxRight,
    /// Arrives on either FX channel; the host knows which from
    /// [`Event::FxChannel`].
    BeatFxOnOff,
}

/// Which part of the jog wheel moved. flx4-midi §2.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JogSurface {
    /// The platter top. The vinyl-mode setting only changes which CC the
    /// hardware sends, so both are reported as `Platter`.
    Platter,
    /// The outer ring.
    Side,
}

/// 14-bit per-deck knobs and faders. flx4-midi §2.3, §2.5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeckControl {
    /// Raw slider value; 0x0000 is +range and 0x3FFF is about −range
    /// (flx4-midi §2.3).
    Tempo,
    Trim,
    EqHigh,
    EqMid,
    EqLow,
    ChannelFader,
    Filter,
}

/// 14-bit controls that belong to no deck. flx4-midi §2.6
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalControl {
    Crossfader,
    HeadphoneMix,
}

/// The Beat FX CH SELECT positions that send a message. The third position
/// sends nothing (flx4-midi §2.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxChannel {
    One,
    Two,
}

/// One decoded controller message.
///
/// `shift` on a button, pad or jog event means the hardware sent the shifted
/// variant: the controller applies SHIFT itself (flx4-midi §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    Button {
        deck: Deck,
        button: Button,
        shift: bool,
        pressed: bool,
    },
    Global {
        button: GlobalButton,
        shift: bool,
        pressed: bool,
    },
    Pad {
        deck: Deck,
        mode: PadMode,
        /// 0..=7, pad 1 is 0.
        pad: u8,
        shift: bool,
        pressed: bool,
    },
    Jog {
        deck: Deck,
        surface: JogSurface,
        /// Ticks since the last message; negative is counter-clockwise.
        delta: i8,
        shift: bool,
    },
    Browse {
        delta: i8,
        shift: bool,
    },
    /// 0..=0x3FFF.
    Deck14 {
        deck: Deck,
        control: DeckControl,
        value: u16,
    },
    /// 0..=0x3FFF.
    Global14 {
        control: GlobalControl,
        value: u16,
    },
    /// Beat FX LEVEL/DEPTH, 7-bit. The hardware sends the same CC with SHIFT
    /// held; the host decides level vs depth from [`Button::Shift`].
    FxLevel {
        value: u8,
    },
    FxChannel {
        channel: FxChannel,
        selected: bool,
    },
}

/// A controller map's input side. Stateful because 14-bit controls arrive as
/// two messages.
pub trait Controller {
    /// Decode one complete MIDI message. Unknown, malformed and incomplete
    /// messages (such as a 14-bit MSB on its own) return `None`.
    fn decode(&mut self, message: &[u8]) -> Option<Event>;
}
