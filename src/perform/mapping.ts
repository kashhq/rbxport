/**
 * Which upstream command a controller event runs: the slow path's table, from
 * `docs/perform/design.md`. Pure, so the whole map is enumerable in a test.
 *
 * Only presses run commands. A release runs nothing, and so does any shift
 * variant the design does not name: a shifted control doing its unshifted
 * job would be a surprise on the hardware, where SHIFT always changes what a
 * control does.
 */
import type { CommandScope } from "@/lib/commands";
import { FIXED_JUMPS, type Action } from "@/lib/shortcuts";
import type { PerformButton, PerformEvent, PerformGlobalButton, PerformPadMode } from "./types";

export interface PerformCommand {
  action: Action;
  scope: CommandScope;
}

/** What a control runs, unshifted and with SHIFT held; null for nothing. */
export interface ShiftPair {
  plain: Action | null;
  shift: Action | null;
}

function fixedJump(beats: number, direction: 1 | -1): Action | null {
  return FIXED_JUMPS.find((jump) => jump.beats === beats && jump.direction === direction)?.action ?? null;
}

function longestFixedJump(direction: 1 | -1): Action | null {
  const longest = FIXED_JUMPS
    .filter((jump) => jump.direction === direction)
    .reduce<(typeof FIXED_JUMPS)[number] | null>((best, jump) => (best === null || jump.beats > best.beats ? jump : best), null);
  return longest?.action ?? null;
}

/**
 * The deck buttons. PFL itself is the host's (the cue bus); PLAY comes here
 * rather than to the engine so that the UI stays the one owner of quantized
 * play. SHIFT + CALL is the longest fixed jump each way: 8 bars.
 */
export const BUTTON_ACTIONS: Readonly<Record<PerformButton, ShiftPair>> = {
  play: { plain: "playPause", shift: null },
  cue: { plain: "cue", shift: null },
  sync: { plain: "sync", shift: null },
  syncLong: { plain: "masterTempo", shift: null },
  pfl: { plain: null, shift: "quantize" },
  loopIn: { plain: "loopIn", shift: null },
  loopOut: { plain: "loopOut", shift: null },
  reloop: { plain: "reloop", shift: null },
  callBack: { plain: "loopHalf", shift: longestFixedJump(-1) },
  callForward: { plain: "loopDouble", shift: longestFixedJump(1) },
};

/** The beat loop pads' lengths, in beats, pad 0 first. */
export const BEAT_LOOP_PAD_BEATS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;

/**
 * Upstream's beat loop commands (`BEAT_LOOP_BEATS` in `Player.tsx`) start at
 * one beat, so the ¼ and ½ beat pads have none.
 */
const BEAT_LOOP_PADS: readonly (Action | null)[] = [
  null, null, "beatLoop1", "beatLoop2", "beatLoop4", "beatLoop8", "beatLoop16", "beatLoop32",
];

/** The beat jump pads: back then forward, 1, 2, 4 and 8 beats. */
export const BEAT_JUMP_PAD_STEPS: readonly { beats: number; direction: 1 | -1 }[] = [
  { beats: 1, direction: -1 }, { beats: 1, direction: 1 },
  { beats: 2, direction: -1 }, { beats: 2, direction: 1 },
  { beats: 4, direction: -1 }, { beats: 4, direction: 1 },
  { beats: 8, direction: -1 }, { beats: 8, direction: 1 },
];

const NOTHING: readonly ShiftPair[] = Array.from({ length: 8 }, () => ({ plain: null, shift: null }));

/**
 * The pads, eight per mode. Key shift is the host's fast path; sampler,
 * keyboard and pad FX are not in milestone 1.
 */
export const PAD_ACTIONS: Readonly<Record<PerformPadMode, readonly ShiftPair[]>> = {
  hotCue: (["A", "B", "C", "D", "E", "F", "G", "H"] as const).map((letter) => ({
    plain: `hotCue${letter}` as const,
    shift: `clearHotCue${letter}` as const,
  })),
  beatLoop: BEAT_LOOP_PADS.map((plain) => ({ plain, shift: null })),
  beatJump: BEAT_JUMP_PAD_STEPS.map(({ beats, direction }) => ({ plain: fixedJump(beats, direction), shift: null })),
  keyShift: NOTHING,
  sampler: NOTHING,
  keyboard: NOTHING,
  padFx1: NOTHING,
  padFx2: NOTHING,
};

/**
 * The browse section. LOAD 1 is upstream's `loadPlayer1`, which Player A
 * registers. Upstream has no "load on Player 2" command (its context menu's
 * item calls the loader directly) and no command for pressing the browse
 * knob (Enter is `loadPlayer1`), so those two run nothing until there is one.
 */
export const GLOBAL_ACTIONS: Readonly<Record<PerformGlobalButton, ShiftPair & { scope: CommandScope }>> = {
  browsePress: { plain: null, shift: null, scope: "main" },
  loadA: { plain: "loadPlayer1", shift: null, scope: "a" },
  loadB: { plain: null, shift: null, scope: "b" },
};

/**
 * Whether the person did something that should have run a command: a press,
 * or a turn of the browse knob. A release or a status change is not.
 */
export function isPress(event: PerformEvent): boolean {
  switch (event.kind) {
    case "connected":
    case "disconnected":
      return false;
    case "browse":
      return Math.trunc(event.delta) !== 0;
    default:
      return event.pressed;
  }
}

function pick(pair: ShiftPair | undefined, shift: boolean): Action | null {
  if (!pair) return null;
  return shift ? pair.shift : pair.plain;
}

/**
 * The commands an event runs, in order: none for a release or an unmapped
 * control, one per detent for the browse knob.
 */
export function commandsFor(event: PerformEvent): PerformCommand[] {
  switch (event.kind) {
    case "connected":
    case "disconnected":
      return [];
    case "button": {
      if (!event.pressed) return [];
      const action = pick(BUTTON_ACTIONS[event.button], event.shift);
      return action ? [{ action, scope: event.deck }] : [];
    }
    case "pad": {
      if (!event.pressed) return [];
      const action = pick(PAD_ACTIONS[event.mode][event.pad], event.shift);
      return action ? [{ action, scope: event.deck }] : [];
    }
    case "browse": {
      if (event.shift) return [];
      const action: Action = event.delta > 0 ? "moveDown" : "moveUp";
      return Array.from({ length: Math.trunc(Math.abs(event.delta)) }, () => ({ action, scope: "main" }));
    }
    case "global": {
      if (!event.pressed) return [];
      const row = GLOBAL_ACTIONS[event.button];
      const action = pick(row, event.shift);
      return action ? [{ action, scope: row.scope }] : [];
    }
  }
}
