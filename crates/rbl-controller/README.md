# rbl-controller

[Crate index](../README.md) · [FLX4 MIDI reference](../../docs/perform/flx4-midi.md) · [Fork](../../docs/fork.md)

Turns DJ-controller MIDI into typed events and an LED model into MIDI. One map per controller; the Pioneer DDJ-FLX4 is the first. Device I/O (`midir`, the keep-alive timer, hot-plug) belongs to `src-tauri/src/perform`.

## Start here

Start with `Event`, `Controller` and the control enums in [`src/lib.rs`](src/lib.rs), then the FLX4 decoder in [`src/flx4/mod.rs`](src/flx4/mod.rs). Every note and CC lives in [`src/flx4/map.rs`](src/flx4/map.rs) with the section of [`flx4-midi.md`](../../docs/perform/flx4-midi.md) it comes from.

Read [`src/flx4/tests.rs`](src/flx4/tests.rs) for the byte-level cases: one row per input in §2 and §4.

## Code map

| File in `src/` | Responsibility |
| --- | --- |
| [`lib.rs`](src/lib.rs) | `Event` and its control enums, the `Controller` trait. |
| [`flx4/mod.rs`](src/flx4/mod.rs) | FLX4 decoder: status/channel dispatch, 14-bit latching, jog and browse encodings. |
| [`flx4/map.rs`](src/flx4/map.rs) | FLX4 note/CC constants and the keep-alive SysEx, each cited. |
| [`flx4/leds.rs`](src/flx4/leds.rs) | `Leds` model and the diffing renderer, shift-layer mirroring, shutdown blackout. |
| [`flx4/tests.rs`](src/flx4/tests.rs) | Byte-level decode and LED tests. |

## Contracts

- `decode` takes one complete message as `midir` delivers it. It never panics; unknown, malformed and incomplete messages return `None` (logged at `trace`).
- 14-bit controls emit on the LSB with `(msb << 7) | lsb`. An MSB alone emits nothing; an LSB with no MSB yet uses 0.
- `Leds::messages(Some(previous))` returns only what changed, so the host can call it on every state change. `None` writes every LED. The host must send `Flx4::KEEPALIVE` at startup and every `Flx4::KEEPALIVE_INTERVAL_MS`.
- Evidence: the FLX4 map is `[OBS mapping]` (Mixxx's community mapping), not a capture of the hardware. `[ASSUME]` marks inferences in `map.rs`. A change confirmed on the physical controller should say so in the reference and the constant's comment.

## Add a controller

1. Write its reference under `docs/perform/`, sectioned like `flx4-midi.md`, with evidence tags.
2. Add `src/<model>/` with `map.rs` (cited constants), a decoder implementing `Controller`, and its LED renderer.
3. Reuse `Event` where the control means the same thing; add a variant only for a control no existing one describes.
4. Add a byte-level test table covering every input row of the reference.

## Run focused checks

From the repository root:

```sh
cargo test -p rbl-controller
cargo clippy -p rbl-controller --all-targets -- -D warnings
```

Follow the [test guide](../../docs/development/testing.md) for broader checks.
