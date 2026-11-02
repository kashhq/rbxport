# rbxport (performance fork)

A tracking fork of chrisle/rbxport that adds performance mode: a DJ controller
(Pioneer DDJ-FLX4 first) drives the two decks, with headphone cueing on the
controller's sound card. Read `docs/fork.md` first: it says what is ours, what
is upstream's, and how upstream is synced.

## Stack
Conventions: architecture, delivery, observability, backend, frontend, frontend-react, desktop

Upstream's own guides govern code that touches upstream's crates and views:
`docs/development/architecture.md`, `conventions.md`, `testing.md`. Where a
house convention and an upstream convention disagree inside an upstream file,
upstream wins (it has to merge).

## Architecture

Upstream: React (`src/`) → typed IPC (`src/ipc/`) → Tauri commands
(`src-tauri/`) → Tauri-free `rbl-*` crates. Playback is `crates/rbl-deck`
(two decks, one audio callback, lock-free control), driven by commands in
`src-tauri/src/commands.rs` and the `usePlayback` store. Hot cues, beat jump,
quantize and sync are **frontend** logic registered as commands
(`src/lib/commands.ts`, `runCommand(action, {scope: "a"|"b"})`); the engine
only knows play, pause, seek, loop, tempo, key shift and the mixer strip.

Ours, additive on top:

- `crates/rbl-controller`: pure functions. MIDI bytes → `ControllerEvent`,
  LED state → MIDI bytes, one map per controller. No Tauri, no device I/O.
- `src-tauri/src/perform/`: owns the `midir` connection and the MIDI thread.
  Two paths out of it:
  - **fast path**: events that are engine calls already (play/pause, jog,
    tempo fader, channel fader, EQ, trim, crossfader) go straight to the
    `rbl-deck` `Engine`, never through the UI. Jog and faders must not wait
    on a webview round trip.
  - **slow path**: everything the frontend owns (hot cues, loops, beat jump,
    sync, quantize, browse, load) is emitted as a `perform:event` and the UI
    bridge runs the matching `runCommand`.
- `src/perform/`: the bridge. It listens to `perform:event`, calls
  `runCommand` with the deck scope, and pushes the LED-relevant state
  (hot cue slots, loop, sync, master tempo, pad mode) back through the
  `perform_leds` command.
- Cue bus: `rbl-deck` renders a second stereo pair (the headphone mix) into
  output lanes 3–4 when the device has them. Marked `FLX:` edits.

## Conventions

- **Additive first.** New behaviour goes in our paths (`docs/fork.md` table).
  An upstream file is edited only for a hook line, marked `FLX:` on the line,
  and listed in `docs/fork.md` in the same change.
- **Never reformat, rename or move code in an upstream file.**
- **Upstream's lints are ours.** Clippy `perf` denied, `pedantic` on,
  `unwrap`/`expect`/`panic` denied outside tests, warnings are errors;
  ESLint bans `setInterval` and stray `console.log`; strings go through
  `useTranslation()`; the initial JS chunk has a 150 KB gzip gate, so
  performance UI is lazy-loaded.
- **Platform gating.** `midir` code is `#[cfg(any(target_os = "macos",
  windows, target_os = "linux"))]`, as `src-tauri/src/rx3_link.rs` does, so
  the Linux clippy lane builds.
- **Controller maps are data with tests.** Every FLX4 note/CC in
  `rbl-controller` cites `docs/perform/flx4-midi.md`; a mapping change comes
  with a byte-level test.
- **Evidence tags.** Keep upstream's `[OBS]`/`[ASSUME]`/`[UNKNOWN]` habit
  for anything about hardware behaviour: say whether it came from the Mixxx
  mapping, a MIDI capture, or the physical FLX4.
- Logging: the `perform` module logs device connect/disconnect, map
  selection, and fast/slow-path routing at `debug`, never per MIDI message
  at `info`.

## Commands

| What | Command |
| --- | --- |
| Install | `pnpm install --frozen-lockfile` (Node 24, pnpm 10.17.1) |
| Desktop dev | `pnpm dev` |
| Browser dev (mock backend) | `pnpm dev:web` |
| Frontend checks | `pnpm typecheck && pnpm lint && pnpm test` |
| Bundle gate | `pnpm budget` |
| Controller crate | `cargo test -p rbl-controller` |
| Deck engine | `cargo test -p rbl-deck` |
| App shell | `cargo clippy -p rbxport --all-targets -- -D warnings` |
| Sync upstream | `/fork-sync` (claude-kit) |

## Workflow
<!-- ORCHESTRATION-OVERRIDE: claudebot agents skip this section.
     Your agent definition governs your workflow. -->

- Work on a branch, PR to `main`. Builds and test runs go through the
  `build:build-runner` agent, not the main context.
- Every PR that edits an upstream file updates `docs/fork.md`.
- Decisions go in `docs/decisions.md` in the same PR as the code.
- Upstream syncs are their own PR, merged with a merge commit, never squashed.
