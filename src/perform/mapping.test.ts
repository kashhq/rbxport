/**
 * The controller's slow-path map, control by control. A wrong row here is a
 * pad that clears a hot cue instead of calling it, so every row is pinned,
 * shift variants and releases included.
 */
import { describe, expect, it } from "vitest";

import { beatLoopLength, fixedJump, type Action } from "@/lib/shortcuts";
import {
  BEAT_JUMP_PAD_STEPS, BEAT_LOOP_PAD_BEATS, BUTTON_ACTIONS, commandsFor, GLOBAL_ACTIONS, isPress, PAD_ACTIONS,
} from "./mapping";
import type { PerformButton, PerformDeck, PerformEvent, PerformPadMode } from "./types";

const button = (deck: PerformDeck, name: PerformButton, shift = false, pressed = true): PerformEvent =>
  ({ kind: "button", deck, button: name, shift, pressed });
const pad = (deck: PerformDeck, mode: PerformPadMode, index: number, shift = false, pressed = true): PerformEvent =>
  ({ kind: "pad", deck, mode, pad: index, shift, pressed });

describe("deck buttons", () => {
  const rows: [PerformButton, boolean, Action | null][] = [
    ["play", false, "playPause"],
    ["play", true, null],
    ["cue", false, "cue"],
    ["cue", true, null],
    ["sync", false, "sync"],
    ["sync", true, null],
    ["syncLong", false, "masterTempo"],
    ["syncLong", true, null],
    ["pfl", false, null],
    ["pfl", true, "quantize"],
    ["loopIn", false, "loopIn"],
    ["loopIn", true, null],
    ["loopOut", false, "loopOut"],
    ["loopOut", true, null],
    ["reloop", false, "reloop"],
    ["reloop", true, null],
    ["callBack", false, "loopHalf"],
    ["callBack", true, "jumpBackward8Bars"],
    ["callForward", false, "loopDouble"],
    ["callForward", true, "jumpForward8Bars"],
  ];

  it("covers every button both ways", () => {
    const names = Object.keys(BUTTON_ACTIONS).sort();
    expect([...new Set(rows.map(([name]) => name))].sort()).toEqual(names);
    expect(rows).toHaveLength(names.length * 2);
  });

  it.each(rows)("%s (shift %s) runs %s on its own deck", (name, shift, action) => {
    for (const deck of ["a", "b"] as const) {
      expect(commandsFor(button(deck, name, shift))).toEqual(action ? [{ action, scope: deck }] : []);
    }
  });

  it("ignores every release", () => {
    for (const name of Object.keys(BUTTON_ACTIONS) as PerformButton[]) {
      expect(commandsFor(button("a", name, false, false))).toEqual([]);
      expect(commandsFor(button("a", name, true, false))).toEqual([]);
    }
  });

  it("puts SHIFT + CALL on the longest fixed jump each way", () => {
    expect(fixedJump("jumpBackward8Bars")).toEqual({ beats: 32, direction: -1 });
    expect(fixedJump("jumpForward8Bars")).toEqual({ beats: 32, direction: 1 });
  });
});

describe("pads", () => {
  it("calls hot cues A to H, and clears them with shift", () => {
    "ABCDEFGH".split("").forEach((letter, index) => {
      expect(commandsFor(pad("b", "hotCue", index))).toEqual([{ action: `hotCue${letter}`, scope: "b" }]);
      expect(commandsFor(pad("b", "hotCue", index, true))).toEqual([{ action: `clearHotCue${letter}`, scope: "b" }]);
    });
  });

  it("starts upstream's beat loops, with nothing under one beat", () => {
    expect(PAD_ACTIONS.beatLoop.map((row) => row.plain)).toEqual([
      null, null, "beatLoop1", "beatLoop2", "beatLoop4", "beatLoop8", "beatLoop16", "beatLoop32",
    ]);
    PAD_ACTIONS.beatLoop.forEach((row, index) => {
      if (row.plain) expect(beatLoopLength(row.plain)).toBe(BEAT_LOOP_PAD_BEATS[index]);
      expect(commandsFor(pad("a", "beatLoop", index, true))).toEqual([]);
    });
    expect(commandsFor(pad("a", "beatLoop", 4))).toEqual([{ action: "beatLoop4", scope: "a" }]);
  });

  it("jumps by upstream's fixed jumps: back then forward, 1 to 8 beats", () => {
    expect(PAD_ACTIONS.beatJump.map((row) => row.plain)).toEqual([
      "jumpBackward1Beat", "jumpForward1Beat",
      "jumpBackward2Beats", "jumpForward2Beats",
      "jumpBackward4Beats", "jumpForward4Beats",
      "jumpBackward8Beats", "jumpForward8Beats",
    ]);
    PAD_ACTIONS.beatJump.forEach((row, index) => {
      expect(row.plain && fixedJump(row.plain)).toEqual(BEAT_JUMP_PAD_STEPS[index]);
      expect(commandsFor(pad("a", "beatJump", index, true))).toEqual([]);
    });
  });

  it.each(["keyShift", "sampler", "keyboard", "padFx1", "padFx2"] as const)("leaves %s pads to the host or to later", (mode) => {
    for (let index = 0; index < 8; index += 1) {
      expect(commandsFor(pad("a", mode, index))).toEqual([]);
      expect(commandsFor(pad("a", mode, index, true))).toEqual([]);
    }
  });

  it("has eight rows per mode and ignores releases and pads past eight", () => {
    for (const [mode, rows] of Object.entries(PAD_ACTIONS) as [PerformPadMode, unknown[]][]) {
      expect(rows).toHaveLength(8);
      expect(commandsFor(pad("a", mode, 0, false, false))).toEqual([]);
      expect(commandsFor(pad("a", mode, 8))).toEqual([]);
    }
  });
});

describe("browse", () => {
  it("moves the main list one row per detent", () => {
    expect(commandsFor({ kind: "browse", delta: 3, shift: false })).toEqual(
      Array.from({ length: 3 }, () => ({ action: "moveDown", scope: "main" })),
    );
    expect(commandsFor({ kind: "browse", delta: -2, shift: false })).toEqual(
      Array.from({ length: 2 }, () => ({ action: "moveUp", scope: "main" })),
    );
    expect(commandsFor({ kind: "browse", delta: 0, shift: false })).toEqual([]);
  });

  it("runs nothing with shift", () => {
    expect(commandsFor({ kind: "browse", delta: 1, shift: true })).toEqual([]);
  });

  it.each([
    ["loadA", false, "loadPlayer1", "a"],
    ["loadA", true, null, "a"],
    ["loadB", false, null, "b"],
    ["loadB", true, null, "b"],
    ["browsePress", false, null, "main"],
    ["browsePress", true, null, "main"],
  ] as const)("%s (shift %s) runs %s", (name, shift, action, scope) => {
    expect(GLOBAL_ACTIONS[name].scope).toBe(scope);
    expect(commandsFor({ kind: "global", button: name, shift, pressed: true })).toEqual(action ? [{ action, scope }] : []);
    expect(commandsFor({ kind: "global", button: name, shift, pressed: false })).toEqual([]);
  });
});

describe("isPress", () => {
  it("counts presses and turns, not releases or status", () => {
    expect(isPress(button("a", "play"))).toBe(true);
    expect(isPress(button("a", "play", false, false))).toBe(false);
    expect(isPress({ kind: "browse", delta: -1, shift: false })).toBe(true);
    expect(isPress({ kind: "browse", delta: 0, shift: false })).toBe(false);
    expect(isPress({ kind: "connected", name: "DDJ-FLX4" })).toBe(false);
    expect(isPress({ kind: "disconnected" })).toBe(false);
  });
});
