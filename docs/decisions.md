# Decisions

Dated, newest last. Why / Decision / Consequence, kept short. Standing rules
live in `AGENTS.md`; the fork's upstream relationship in `docs/fork.md`.

## 2026-11-02 — Fork rbxport rather than build a DJ app or extend Mixxx

Why: rbxport already has the three expensive parts (own BPM/grid/key/waveform
analysis in `rbl-analysis`, a lock-free two-deck engine with key lock and a
mixer strip in `rbl-deck`, a rekordbox-faithful 2-player UI). Upstream rules
controllers out of scope (#606, #690), so it will never ship this.
Decision: tracking fork, additive code, `FLX:`-marked hooks, merge syncs.
Consequence: we live with upstream's lints and a 3,000-line `Player.tsx` we
must not edit; anything it lacks is added as a new registered command.

## 2026-11-02 — Track upstream `main`, not `dev`

Why: upstream's CI merges every PR into `dev` continuously; `main` moves at
releases. Syncing `dev` would mean merging unreleased, partly validated work.
Decision: `docs/fork.md` records `main`; sync at upstream release tags.
Consequence: we are at most one release behind; a needed `dev` fix is
cherry-picked with `-x` and noted in `docs/fork.md`.

## 2026-11-02 — Controller input splits into a fast path and a slow path

Why: hot cues, beat jump, quantize and sync are frontend logic in upstream
(`useHotCues`, `deckQuantize.ts`, `sync.ts`), reachable through
`runCommand`. Duplicating them in Rust would fork their behaviour and conflict
with every upstream change to them. But jog, faders and EQ at MIDI rate cannot
afford a Rust → webview → Rust round trip per message.
Decision: `src-tauri/src/perform` sends engine-native controls (jog/scrub,
play/pause, tempo, faders, EQ, trim, crossfader) straight to `rbl_deck::Engine`;
everything else is emitted as `perform:event` and `src/perform` runs the
matching `runCommand(action, {scope})`.
Consequence: two code paths to keep consistent; the bridge must report UI-owned
state (cue slots, loop, sync) back to Rust for LED feedback.

## 2026-11-02 — Controller logic is a Tauri-free crate

Why: upstream's architecture keeps every protocol and format in an `rbl-*`
crate with byte-level tests, and `midir` device I/O is OS-gated.
Decision: `crates/rbl-controller` holds the FLX4 map (bytes ⇄ typed events,
LED model) as pure functions with tests; `src-tauri/src/perform` owns the
device. The map is derived from Mixxx's GPL-2 FLX4 mapping (same licence as
rbxport) and recorded in `docs/perform/flx4-midi.md`.
Consequence: a second controller is a new map module and a test file, no app
changes.

## 2026-11-02 — FLX4 LEDs render as a fixed frame and diff

Why: pad LEDs are addressed by the shown mode's notes, so a mode change must
clear the old mode's pads; ad-hoc per-field diffing misses those clears.
Decision: `Leds` renders every driven address in a fixed order (pads lit only
for `pad_mode`), and `messages(previous)` diffs two frames elementwise.
Consequence: diffing re-sends the jog-ring "loaded" animation only when
`loaded` changes; to retrigger it on a second load the host clears `loaded`
for one frame. Shutdown writes the all-off frame plus the sampler pads.

## 2026-11-02 — Bridge IPC lives in `src/ipc/perform.ts`; UI LED state via a store

Why: adding `perform_*` to upstream's `Backend` means editing its types, client
and mock. Hot cue slots, pad mode (Player) and SYNC (`App.tsx`) are component
state; CUE's preview ends on the key's keyup, not a command.
Decision: a standalone IPC module, no-op outside Tauri. `src/perform/deckState.ts`
is a tiny store Player and App publish to from one-call `FLX:` hook lines;
master tempo/CUE come from `deck:tick`, Q from `deckQuantize.ts` per tick. Ours:
`cueRelease` and `loadPlayer2` actions. Unnamed shift variants run nothing.
Consequence: Player's pad row is only "cue" or "grid", so `padMode` is never
beatJump/beatLoop/keyShift from the UI; the host owns those pages.

## 2026-11-02 — The cue bus taps the strip, and lives behind `Option`

Why: PFL must be post-EQ, pre-fader; running the EQ a second time per deck
doubles the strip's cost, and a separate cue renderer would fork the mixer.
Decision: `Channel::process` adds its post-EQ sample into an optional `tap`
(the sink's cue chunk directly, no extra engine buffers); `Render` takes
`Option<&mut [f32]>`, `None` below four lanes, so a stereo device computes
nothing cue-related. The cue mix reads the post-limiter, post-clamp master.
Consequence: `CueMixer` owns a second `Limiter` on the PFL sum, driven by the
master's `LimiterSettings` (one switch for both; off, the cue only clamps).
It runs before the master is blended in, not after the mix as first asked:
limiting the blend delays the master twice and combs a deck that is both cued
and live.

## 2026-11-02 — Widening to four output lanes is opt-in (`StreamWish::cue`)

Why: a four-out audio interface's lanes 3–4 are not necessarily headphones;
widening every device would send the cue mix to someone's monitors or a
second zone. Only a connected controller says where the headphones are.
Decision: `StreamWish` gains `cue: bool`, default false. False keeps the
device's default lanes and hands the render `None`, as upstream; true widens
to the fewest lanes ≥4 at the same format and rate. `set_audio_config` keeps
`cue` via `..player.wish()`. The perform host sets it on controller connect.
Consequence: setting it drops the engine (`Player::set_wish` returns true), so
the host emits `deck:reset` as `set_audio_config` does.

## 2026-11-02 — The perform host owns pad mode, tempo range and engine replay

Why: the FLX4 switches pad pages itself and the UI never reports them; the UI's
tempo range is frontend-only; a cue-bus `set_wish` rebuilds the engine at defaults.
Decision: `HostState` tracks the page from mode-button presses (LEDs prefer it);
the slider is a fixed ±6 % (`TEMPO_RANGE`) with soft takeover (inactive after a
connect or rebuild until within 0.001 of, or across, the deck's tempo); mixer
values and PFL, never tempo, are replayed onto any engine not seen (by `Weak`).
Consequence: SHIFT+SYNC's range never reaches the slider; masterTempo/quantize left the LED wire.

## 2026-11-02 — The cue bus follows the audio output, not the MIDI link

Why: lanes 3–4 are headphones only on the FLX4's own sound card; a controller
plugged in while playing through speakers must not widen them, and a dropped
MIDI send must not rebuild a playing engine.
Decision: `perform-out` sets `cue` to "the output the Player opens is named
DDJ-FLX4", checked on connect, on engine rebuild and on a `Player::device` change,
calling `set_wish` only on a difference. Disconnect leaves the wish alone.
Consequence: a system-default switch with no rebuild is seen at the next rebuild.
