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

## 2026-11-02 — Bridge IPC lives in `src/ipc/perform.ts`; some LEDs are dark

Why: adding `perform_*` to upstream's `Backend` means editing its types, client
and mock. Hot cue slots (Player's `cues` prop), SYNC (`App.tsx` `synced` state)
and pad mode (Player's `padMode`) are component state we may not edit.
Decision: a standalone IPC module, no-op outside Tauri. LEDs send master tempo
and CUE from `deck:tick`, Q from `deckQuantize.ts` re-read per tick; the rest
dark. Unshifted-only rows: a shift variant the design does not name runs nothing.
Consequence: lighting slots/SYNC needs an `FLX:` publish line; controller CUE
cannot end a preview (upstream ends it on keyup, not a command).
