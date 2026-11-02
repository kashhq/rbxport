//! The wire contract with the bridge, field for field `src/perform/types.ts`.
//!
//! Every name here is what the TypeScript union spells; the serde tests at
//! the bottom pin the JSON shape, so a rename on either side fails a test
//! rather than silently dropping events in the bridge.

use rbl_controller::{Button, Deck, GlobalButton, PadMode};
use serde::{Deserialize, Serialize};

/// `PerformDeck`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeckDto {
    A,
    B,
}

impl From<Deck> for DeckDto {
    fn from(deck: Deck) -> Self {
        match deck {
            Deck::A => Self::A,
            Deck::B => Self::B,
        }
    }
}

/// `PerformButton`: the deck buttons the bridge runs commands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonDto {
    Play,
    Cue,
    Sync,
    SyncLong,
    Pfl,
    LoopIn,
    LoopOut,
    Reloop,
    CallBack,
    CallForward,
}

impl ButtonDto {
    /// The slow-path name of a deck button; `None` for the buttons the host
    /// keeps (SHIFT, pad-mode buttons, platter touch).
    pub fn of(button: Button) -> Option<Self> {
        Some(match button {
            Button::Play => Self::Play,
            Button::Cue => Self::Cue,
            Button::Sync => Self::Sync,
            Button::SyncLong => Self::SyncLong,
            Button::Pfl => Self::Pfl,
            Button::LoopIn => Self::LoopIn,
            Button::LoopOut => Self::LoopOut,
            Button::Reloop => Self::Reloop,
            Button::CueLoopCallBack => Self::CallBack,
            Button::CueLoopCallForward => Self::CallForward,
            Button::Shift | Button::PadMode(_) | Button::JogTouch => return None,
        })
    }
}

/// `PerformPadMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PadModeDto {
    HotCue,
    BeatJump,
    BeatLoop,
    Sampler,
    Keyboard,
    PadFx1,
    PadFx2,
    KeyShift,
}

impl From<PadMode> for PadModeDto {
    fn from(mode: PadMode) -> Self {
        match mode {
            PadMode::HotCue => Self::HotCue,
            PadMode::BeatJump => Self::BeatJump,
            PadMode::BeatLoop => Self::BeatLoop,
            PadMode::Sampler => Self::Sampler,
            PadMode::Keyboard => Self::Keyboard,
            PadMode::PadFx1 => Self::PadFx1,
            PadMode::PadFx2 => Self::PadFx2,
            PadMode::KeyShift => Self::KeyShift,
        }
    }
}

/// `PerformGlobalButton`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalButtonDto {
    BrowsePress,
    LoadA,
    LoadB,
}

impl GlobalButtonDto {
    /// `None` for the Beat FX buttons, which milestone 1 leaves alone.
    pub fn of(button: GlobalButton) -> Option<Self> {
        match button {
            GlobalButton::BrowsePress => Some(Self::BrowsePress),
            GlobalButton::Load(Deck::A) => Some(Self::LoadA),
            GlobalButton::Load(Deck::B) => Some(Self::LoadB),
            GlobalButton::BeatFxSelect
            | GlobalButton::BeatFxLeft
            | GlobalButton::BeatFxRight
            | GlobalButton::BeatFxOnOff => None,
        }
    }
}

/// `PerformEvent`, emitted as `perform:event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PerformEventDto {
    Connected {
        name: String,
    },
    Disconnected,
    Button {
        deck: DeckDto,
        button: ButtonDto,
        shift: bool,
        pressed: bool,
    },
    Pad {
        deck: DeckDto,
        mode: PadModeDto,
        pad: u8,
        shift: bool,
        pressed: bool,
    },
    Browse {
        delta: i8,
        shift: bool,
    },
    Global {
        button: GlobalButtonDto,
        shift: bool,
        pressed: bool,
    },
}

/// `PerformStatus`, `perform_status`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PerformStatusDto {
    pub connected: Option<String>,
}

/// The pad modes the UI can report as showing (`PerformDeckLeds.padMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiPadModeDto {
    HotCue,
    BeatJump,
    BeatLoop,
    KeyShift,
}

impl From<UiPadModeDto> for PadMode {
    fn from(mode: UiPadModeDto) -> Self {
        match mode {
            UiPadModeDto::HotCue => Self::HotCue,
            UiPadModeDto::BeatJump => Self::BeatJump,
            UiPadModeDto::BeatLoop => Self::BeatLoop,
            UiPadModeDto::KeyShift => Self::KeyShift,
        }
    }
}

/// `PerformDeckLeds`. `masterTempo` and `quantize` arrive too but are not
/// declared: the FLX4 has no LED for either that anyone has mapped
/// (flx4-midi §3.1), and serde skips fields it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformDeckLedsDto {
    pub hot_cues: Vec<bool>,
    pub sync: bool,
    pub cue: bool,
    pub pad_mode: Option<UiPadModeDto>,
}

/// `PerformLeds`, `perform_leds`'s argument.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct PerformLedsDto {
    pub a: PerformDeckLedsDto,
    pub b: PerformDeckLedsDto,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    fn wire(event: &PerformEventDto) -> serde_json::Value {
        serde_json::to_value(event).unwrap()
    }

    #[test]
    fn connection_events_match_the_bridge() {
        assert_eq!(
            wire(&PerformEventDto::Connected { name: "DDJ-FLX4".to_owned() }),
            json!({"kind": "connected", "name": "DDJ-FLX4"})
        );
        assert_eq!(wire(&PerformEventDto::Disconnected), json!({"kind": "disconnected"}));
    }

    #[test]
    fn a_button_event_matches_the_bridge() {
        let event = PerformEventDto::Button {
            deck: DeckDto::B,
            button: ButtonDto::SyncLong,
            shift: false,
            pressed: true,
        };
        assert_eq!(
            wire(&event),
            json!({"kind": "button", "deck": "b", "button": "syncLong", "shift": false, "pressed": true})
        );
    }

    #[test]
    fn a_pad_event_matches_the_bridge() {
        let event = PerformEventDto::Pad {
            deck: DeckDto::A,
            mode: PadModeDto::HotCue,
            pad: 0,
            shift: false,
            pressed: true,
        };
        assert_eq!(
            wire(&event),
            json!({"kind": "pad", "deck": "a", "mode": "hotCue", "pad": 0, "shift": false, "pressed": true})
        );
    }

    #[test]
    fn browse_and_global_events_match_the_bridge() {
        assert_eq!(
            wire(&PerformEventDto::Browse { delta: -2, shift: true }),
            json!({"kind": "browse", "delta": -2, "shift": true})
        );
        assert_eq!(
            wire(&PerformEventDto::Global { button: GlobalButtonDto::LoadB, shift: false, pressed: false }),
            json!({"kind": "global", "button": "loadB", "shift": false, "pressed": false})
        );
    }

    #[test]
    fn every_name_is_the_typescript_spelling() {
        let buttons = [
            (ButtonDto::Play, "play"),
            (ButtonDto::Cue, "cue"),
            (ButtonDto::Sync, "sync"),
            (ButtonDto::SyncLong, "syncLong"),
            (ButtonDto::Pfl, "pfl"),
            (ButtonDto::LoopIn, "loopIn"),
            (ButtonDto::LoopOut, "loopOut"),
            (ButtonDto::Reloop, "reloop"),
            (ButtonDto::CallBack, "callBack"),
            (ButtonDto::CallForward, "callForward"),
        ];
        for (button, name) in buttons {
            assert_eq!(serde_json::to_value(button).unwrap(), json!(name));
        }
        let modes = [
            (PadModeDto::HotCue, "hotCue"),
            (PadModeDto::BeatJump, "beatJump"),
            (PadModeDto::BeatLoop, "beatLoop"),
            (PadModeDto::Sampler, "sampler"),
            (PadModeDto::Keyboard, "keyboard"),
            (PadModeDto::PadFx1, "padFx1"),
            (PadModeDto::PadFx2, "padFx2"),
            (PadModeDto::KeyShift, "keyShift"),
        ];
        for (mode, name) in modes {
            assert_eq!(serde_json::to_value(mode).unwrap(), json!(name));
        }
        let globals = [
            (GlobalButtonDto::BrowsePress, "browsePress"),
            (GlobalButtonDto::LoadA, "loadA"),
            (GlobalButtonDto::LoadB, "loadB"),
        ];
        for (button, name) in globals {
            assert_eq!(serde_json::to_value(button).unwrap(), json!(name));
        }
    }

    #[test]
    fn status_is_a_name_or_null() {
        assert_eq!(
            serde_json::to_value(PerformStatusDto { connected: None }).unwrap(),
            json!({"connected": null})
        );
        assert_eq!(
            serde_json::to_value(PerformStatusDto { connected: Some("DDJ-FLX4".to_owned()) }).unwrap(),
            json!({"connected": "DDJ-FLX4"})
        );
    }

    #[test]
    fn the_bridge_s_led_report_parses() {
        let deck = |pad_mode: serde_json::Value| {
            json!({
                "hotCues": [true, false, false, false, false, false, false, true],
                "sync": true,
                "masterTempo": false,
                "quantize": true,
                "cue": false,
                "padMode": pad_mode,
            })
        };
        let leds: PerformLedsDto =
            serde_json::from_value(json!({"a": deck(json!("hotCue")), "b": deck(json!(null))})).unwrap();
        assert_eq!(leds.a.pad_mode, Some(UiPadModeDto::HotCue));
        assert_eq!(leds.b.pad_mode, None);
        assert!(leds.a.sync);
        assert_eq!(leds.a.hot_cues.len(), 8);
        assert!(serde_json::from_value::<PerformLedsDto>(json!({"a": deck(json!("sampler")), "b": deck(json!(null))})).is_err());
    }
}
