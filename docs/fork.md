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

- `src-tauri/src/lib.rs`: the `perform` module line, its managed state, and
  its commands in `generate_handler!`.
- `src/app/App.tsx`: two `FLX:` lines: the lazy `PerformBridge` import
  after the other `lazy(...)` dialogs, and `<Suspense fallback={null}><PerformBridge /></Suspense>`
  right after `<ConfirmHost />` in the render.
- `crates/README.md`: a `## Performance (fork)` table listing
  `rbl-controller`, marked `FLX:` on its heading.
- `crates/rbl-deck/src/sink.rs`, `lib.rs`, `mixer.rs`: the cue (headphone)
  bus — a second stereo pair rendered to output lanes 3–4. (Planned; update
  when landed.)

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
