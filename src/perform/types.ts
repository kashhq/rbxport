/**
 * The wire contract between the controller host (`src-tauri/src/perform/`)
 * and the bridge. The host emits `PerformEvent` as the `perform:event` Tauri
 * event and answers `perform_status` and `perform_leds`; the Rust side's
 * serde types must stay field-for-field identical to these.
 *
 * Only the slow path arrives here (see `docs/perform/design.md`): jog,
 * faders, EQ, key shift and the cue bus never leave Rust.
 */

export type PerformDeck = "a" | "b";

export type PerformButton =
  | "play" | "cue" | "sync" | "syncLong" | "pfl"
  | "loopIn" | "loopOut" | "reloop" | "callBack" | "callForward";

export type PerformPadMode =
  | "hotCue" | "beatJump" | "beatLoop" | "sampler" | "keyboard" | "padFx1" | "padFx2" | "keyShift";

export type PerformGlobalButton = "browsePress" | "loadA" | "loadB";

export type PerformEvent =
  | { kind: "connected"; name: string }
  | { kind: "disconnected" }
  | { kind: "button"; deck: PerformDeck; button: PerformButton; shift: boolean; pressed: boolean }
  // `pad` is 0..7 in the FLX4's own pad order.
  | { kind: "pad"; deck: PerformDeck; mode: PerformPadMode; pad: number; shift: boolean; pressed: boolean }
  // Detents turned since the last event; positive is down the list.
  | { kind: "browse"; delta: number; shift: boolean }
  | { kind: "global"; button: PerformGlobalButton; shift: boolean; pressed: boolean };

/** `perform_status`'s answer: the connected controller's port name, or null. */
export interface PerformStatus {
  connected: string | null;
}

/**
 * Only what the host lights. Master tempo and Q have no LED anyone has
 * mapped on the FLX4 (`docs/perform/flx4-midi.md` §3.1), so they are not sent.
 */
export interface PerformDeckLeds {
  /** Slot A..H set, in pad order; always eight long. */
  hotCues: boolean[];
  sync: boolean;
  cue: boolean;
  /** The pad mode the UI is showing, or null when it has no opinion. */
  padMode: "hotCue" | "beatJump" | "beatLoop" | "keyShift" | null;
}

export interface PerformLeds {
  a: PerformDeckLeds;
  b: PerformDeckLeds;
}
