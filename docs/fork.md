# Fork

This repository is a **tracking fork** of [chrisle/rbxport](https://github.com/chrisle/rbxport)
that adds a rekordbox-style performance mode: a DJ controller (first the
Pioneer DDJ-FLX4) drives the two decks, with headphone cueing on the
controller's sound card. Upstream explicitly keeps controllers out of scope
(upstream #606, #690), so this lives here and never goes back as a PR.

We take upstream wholesale, one sync per PR, through claude-kit's `fork-sync`
skill. Syncs are **merges, never rebases**: a merge keeps upstream's SHA as an
ancestor so the next sync finds its own base, and it never rewrites our
history. "Stay rebaseable on upstream" is achieved by keeping our diff
additive, not by rebasing.

Fork marker: `FLX:`. Every edit to an upstream file carries it in a comment on
the changed lines (`git grep -n 'FLX:'` lists every divergence). New files
need no marker; their paths are listed below.

## Where our code lives (additive, never conflicts)

| Path | What |
| --- | --- |
| `crates/rbl-controller/` | Tauri-free controller logic: MIDI bytes ⇄ typed events, LED model, per-controller maps (FLX4 first). |
| `src-tauri/src/perform/` | The app side: device discovery and hot-plug, the MIDI thread, the fast path into `rbl-deck`, the slow path (events to the UI), LED feedback. |
| `src/perform/` | The UI side: the bridge that turns controller events into `runCommand` calls and reports deck state back for LEDs. |
| `docs/perform/` | Design notes and the controller references. |
| `AGENTS.md`, `CLAUDE.md`, `docs/fork.md`, `docs/decisions.md` | Fork context. Upstream has none of these. |

## Divergences in upstream files

Each one is a conflict point at the next sync, so each must be small and
marked. Add to this list in the same change as the edit.

- `src-tauri/src/lib.rs` (3): `mod perform;` after `mod network_labels;`;
  `crate::perform::start(app.handle())` in `setup` after
  `spawn_library_load` (it manages `Arc<Perform>` itself and starts the
  controller watcher); `perform_status, perform_leds` at the end of
  `generate_handler!`.
- `src-tauri/Cargo.toml` (1): the `rbl-controller` path dependency after
  `rbl-deck` (`midir` was already there per platform).
- The channel fader: `crates/rbl-deck/src/mixer.rs` (5) gives
  `ChannelSettings` a `level` (default 1.0) with `set_level`/`level`, and
  `Channel::process` multiplies it into the crossfader gain before
  smoothing, so it sits after the PFL tap. Tested in our `tests/cue.rs`.
- `src/app/App.tsx`: four `FLX:` lines: the `usePublishPerformSync` import
  after the `nativeMenu` import; the lazy `PerformBridge` import after the
  other `lazy(...)` dialogs; `usePublishPerformSync(synced)` right after the
  `synced` state; `<Suspense fallback={null}><PerformBridge /></Suspense>`
  right after `<ConfirmHost />` in the render.
- `src/lib/shortcuts.ts`: two `FLX:` lines in the `Action` union,
  `"loadPlayer2"` after `"loadPlayer1"` and `"cueRelease"` after `"cue"`.
  No key binds either.
- `src/views/player/Player.tsx`: four `FLX:` lines: the `usePublishPerformDeck`
  import after the CSS module import; `usePublishPerformDeck(deck, { cues, padMode })`
  right after `useTrackCues`; in `deckCommands`, `cueRelease` (calls
  `dropCue`, what the cue key's keyup calls) after `cue`, and `loadPlayer2`
  (Player B's twin of `loadPlayer1`) after `loadPlayer1`.
- `crates/README.md`: a `## Performance (fork)` table listing
  `rbl-controller`, marked `FLX:` on its heading.
- The cue (headphone) bus, a second stereo pair on output lanes 3–4, opt-in
  through `StreamWish::cue`; the logic is ours in `crates/rbl-deck/src/cue.rs`
  and `tests/cue.rs`. `FLX:` lines per file:
  - `crates/rbl-deck/src/sink.rs` (17): `Render` takes `Option<&mut [f32]>`
    (the cue pair); `StreamWish` gains `cue: bool`; `configure()` calls
    `cue::widen_for_cue` (widens only when `wish.cue`); `build()` takes the
    flag and writes lanes 3–4 from the cue pair; `NullSink` passes `None`.
  - `crates/rbl-deck/src/mixer.rs` (5 of its 10; the other 5 are the
    channel fader above): `Channel::process` gains a pre-fader `tap`
    (post-EQ samples added into it).
  - `crates/rbl-deck/src/lib.rs` (19): `mod cue`, `CueSettings` export,
    `Engine::cue()`, and the PFL taps plus `CueMixer::finish` (with the
    master's `LimiterSettings`) in the render closure.
  - `src-tauri/src/commands.rs` (1): `set_audio_config` builds its wish with
    `..player.wish()`, so a Preferences change keeps the perform host's `cue`.
  - `crates/rbl-deck/tests/allocations.rs` (6): the measured sink passes a
    cue pair, with deck A PFL'd, so the cue path stays allocation-free.
  - `crates/rbl-deck/examples/stripcost.rs` (1): `None` for the new `tap`;
    `examples/startcost.rs` (1) and `src-tauri/examples/deck_smoke.rs` (1):
    the two-argument `Render` closure.

Rules for an upstream-file edit:

1. Prefer a new file plus a one-line hook over a change in place.
2. No reformatting, no renames, no moving code in upstream files.
3. If upstream's `Player.tsx` or `commands.rs` needs a behaviour we lack,
   add it as a new command/handler registered from our files, not by
   editing their bodies.

## Sync record

| upstream | type | synced | upstream commit |
| --- | --- | --- | --- |
| https://github.com/chrisle/rbxport.git (`main`) | tracking | v1.9.0+ (main 2026-10-18) | b739d369 |

- 2026-11-02: forked at upstream `main` b739d369 ("test: cover the fixed beat
  jumps' seek end to end", 2026-10-18). Upstream's `dev` branch is its
  integration branch and is ahead of `main`; we track `main` (released
  state) and sync when upstream cuts a release.

## Notes

- Upstream's `.gitignore` ignores `AGENTS.md` and `CLAUDE.md`. Ours were
  added with `git add -f` once; a tracked file stays tracked regardless of
  ignore rules, so `.gitignore` itself is not edited.
