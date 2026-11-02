# Performance mode: design

[Fork](../fork.md) · [Decisions](../decisions.md) · [FLX4 MIDI reference](flx4-midi.md)

Performance mode is upstream's 2 PLAYER layout driven by a controller. Nothing
in the player view changes; the controller is one more way of running the
commands the keys and buttons already run, plus a direct line into the audio
engine for the controls that cannot wait for the webview.

## Pieces

| Piece | Path | Knows about |
| --- | --- | --- |
| Map | `crates/rbl-controller` | MIDI bytes ⇄ `Event`; `Leds` → MIDI bytes. Nothing else. |
| Host | `src-tauri/src/perform/` | `midir` ports, hot-plug, the keep-alive, `rbl_deck::Engine`, Tauri events. |
| Bridge | `src/perform/` | `perform:event` → `runCommand`; UI state → `perform_leds`. |

## Routing: fast path or slow path

The rule: if the engine already has the operation and the control is
continuous or latency-sensitive, the host calls the engine. Otherwise the
host emits `perform:event` and the bridge runs the upstream command.

| Control (FLX4) | Path | Target |
| --- | --- | --- |
| PLAY/PAUSE | fast | `Engine::play` / `pause` (the UI's `playPause` is quantize-aware; see below) |
| Platter touch + rotate | fast | `scrub_begin` / `scrub_to_ms` (accumulated) / `scrub_end`: the audible drag is already a scratch primitive |
| Side ring / untouched platter | fast | pitch bend: `set_tempo(base × (1 + k·delta))`, decays to `base` 80 ms after the last tick |
| SHIFT + platter | fast | fast search: `seek_ms` by delta × 150 ms |
| TEMPO slider | fast | `set_tempo(ratio)` with the UI's range; soft takeover on connect |
| TRIM, EQ ×3, channel fader | fast | `MixerSettings` channel: `set_trim`, `set_band`, fader |
| Crossfader | fast | `MixerSettings::set_crossfade` |
| FILTER | fast | needs a per-channel filter in the strip (not in upstream; our `FLX:` addition) |
| HEADPHONES MIX, channel CUE (PFL) | fast | the cue bus (ours, in `rbl-deck`) |
| CUE | slow | `cue` |
| BEAT SYNC / long / SHIFT | slow | `sync` / `masterTempo` / tempo range |
| LOOP IN/OUT, RELOOP, CALL ◀ ▶ | slow | `loopIn`, `loopOut`, `reloop`, `loopHalf`, `loopDouble` |
| Hot cue pads 1–8, SHIFT | slow | `hotCueA`…`hotCueH`, `clearHotCueA`… |
| Beat loop pads | slow | `beatLoop¼`…`beatLoop32` (upstream's `BEAT_LOOP_BEATS`) |
| Beat jump pads | slow | upstream's fixed jumps (`FIXED_JUMPS`) |
| Key shift pads | fast | `set_key_shift(semitones)` |
| BROWSE rotate / press, LOAD 1/2 | slow | `moveDown`/`moveUp`, `loadPlayer1`/`loadPlayer2` (scope `main`) |
| Beat FX | — | not in milestone 1 |
| Sampler, Pad FX, Keyboard, Smart CFX/Fader | — | not in milestone 1 |

**Play/pause and quantize.** Upstream's `playPause` command waits for the
beat when the deck's Q is on. The host's fast-path play is unquantized. To
keep one behaviour, PLAY goes through the **slow** path (`playPause`): a
button press tolerates a webview hop, and the UI stays the single owner of
quantize. Only jog, faders, knobs and the tempo slider are fast.

So the fast path is exactly: jog (scratch, bend, search), tempo slider, the
mixer strip, the cue bus, and key shift. Everything with a beat-grid opinion
is slow.

## State the host needs

- From the engine (`Engine::snapshot`, already polled by the tick thread):
  playing, loaded, loop, tempo, key shift, meters. Enough for play/loop/VU
  LEDs and the jog animation.
- From the UI (`perform_leds` command, called by the bridge when its state
  changes): hot cue slots set, sync on, master tempo on, pad mode, PFL. The
  bridge debounces to one call per change, never per tick.

## Hot-plug and lifecycle

The host polls `midir` ports every 2 s while no controller is connected
(cheap: port enumeration, no I/O), opens the first port whose name matches a
known map, sends the keep-alive at 200 ms, and writes the full LED state. A
disconnect (send error) returns to polling. The keep-alive thread is the
RX3 lease pattern in `src-tauri/src/rx3_link.rs`.

## Not changing

- `Player.tsx`, `useHotCues`, `deckQuantize.ts`, `sync.ts`: untouched. If the
  controller needs an action they lack, it is added as a new command
  registered from `src/perform/`.
- The tick thread and `deck:tick` event: the bridge reads deck state from
  the same `usePlayback` store the view uses.
