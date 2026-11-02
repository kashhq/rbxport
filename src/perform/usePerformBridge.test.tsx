/**
 * @vitest-environment jsdom
 *
 * The bridge end to end, with the host stubbed: a controller event runs the
 * command a key would, on the right deck, and the LEDs are reported once per
 * change rather than once per tick.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { __setBackend } from "@/ipc/client";
import type { Backend, DeckTick, Tick } from "@/ipc/types";
import { __resetPerformDeckState, publishPerformDeckState } from "./deckState";
import type { PerformEvent, PerformLeds } from "./types";
import { usePerformBridge, type PerformBridgeState } from "./usePerformBridge";

const host = vi.hoisted(() => ({
  emit: null as ((event: PerformEvent) => void) | null,
  unlisten: vi.fn(),
  status: { connected: null as string | null },
  leds: [] as PerformLeds[],
}));

vi.mock("@/ipc/perform", () => ({
  performStatus: () => Promise.resolve(host.status),
  performLeds: (leds: PerformLeds) => {
    host.leds.push(leds);
    return Promise.resolve();
  },
  onPerformEvent: (listener: (event: PerformEvent) => void) => {
    host.emit = listener;
    return Promise.resolve(host.unlisten);
  },
}));

const runCommand = vi.hoisted(() => vi.fn(() => true));
vi.mock("@/lib/commands", () => ({ runCommand }));

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let root: Root;
let state: PerformBridgeState;
let tick: ((tick: Tick) => void) | null;

function Probe() {
  state = usePerformBridge();
  return null;
}

const deck = (fields: Partial<DeckTick> = {}): DeckTick => ({
  frames: 0, totalFrames: 0, generation: 0, playing: false, loaded: true, tempo: 1, masterTempo: false,
  keyShift: 0, startInFrames: 0, loopInFrames: 0, loopOutFrames: 0, looping: false, ...fields,
});
const both = (a: Partial<DeckTick>, b: Partial<DeckTick> = {}): Tick => ({
  a: deck(a), b: deck(b), sampleRate: 48_000, peakLeft: 0, peakRight: 0, master: 1, reduction: 0, shiftsKey: true,
});

async function emit(event: PerformEvent) {
  await act(async () => {
    host.emit?.(event);
    await Promise.resolve();
  });
}

beforeEach(async () => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  host.emit = null;
  host.status = { connected: null };
  host.leds = [];
  host.unlisten.mockClear();
  runCommand.mockClear();
  tick = null;
  __setBackend({
    onDeckTick: (listener: (tick: Tick) => void) => {
      tick = listener;
      return () => {
        tick = null;
      };
    },
  } as unknown as Backend);
  localStorage.clear();
  __resetPerformDeckState();
  const element = document.createElement("div");
  document.body.append(element);
  root = createRoot(element);
  await act(async () => {
    root.render(<Probe />);
    await Promise.resolve();
  });
});

afterEach(() => {
  act(() => root.unmount());
  __setBackend(null);
  vi.restoreAllMocks();
});

describe("usePerformBridge", () => {
  it("runs the mapped command with the deck's scope", async () => {
    await emit({ kind: "pad", deck: "b", mode: "hotCue", pad: 2, shift: false, pressed: true });
    await emit({ kind: "button", deck: "a", button: "callBack", shift: true, pressed: true });
    await emit({ kind: "browse", delta: 2, shift: false });
    expect(runCommand.mock.calls).toEqual([
      ["hotCueC", { scope: "b" }],
      ["jumpBackward8Bars", { scope: "a" }],
      ["moveDown", { scope: "main" }],
      ["moveDown", { scope: "main" }],
    ]);
  });

  it("runs nothing on a release but CUE's, which ends the preview", async () => {
    await emit({ kind: "button", deck: "a", button: "play", shift: false, pressed: false });
    expect(runCommand).not.toHaveBeenCalled();
    await emit({ kind: "button", deck: "b", button: "cue", shift: false, pressed: true });
    await emit({ kind: "button", deck: "b", button: "cue", shift: false, pressed: false });
    expect(runCommand.mock.calls).toEqual([["cue", { scope: "b" }], ["cueRelease", { scope: "b" }]]);
  });

  it("reports the hot cue slots, SYNC and pad mode the view publishes", async () => {
    await emit({ kind: "connected", name: "DDJ-FLX4" });
    await act(async () => {
      publishPerformDeckState("a", { hotCues: [true, false, true, false, false, false, false, true], padMode: "hotCue" });
      publishPerformDeckState("b", { synced: true });
      await Promise.resolve();
    });
    expect(host.leds.at(-1)?.a).toMatchObject({
      hotCues: [true, false, true, false, false, false, false, true], sync: false, padMode: "hotCue",
    });
    expect(host.leds.at(-1)?.b).toMatchObject({ sync: true, padMode: null });
    const reports = host.leds.length;
    await act(async () => {
      publishPerformDeckState("b", { synced: true });
      await Promise.resolve();
    });
    expect(host.leds).toHaveLength(reports);
  });

  it("warns once per unmapped shape, whichever deck", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    await emit({ kind: "pad", deck: "a", mode: "sampler", pad: 0, shift: false, pressed: true });
    await emit({ kind: "pad", deck: "b", mode: "sampler", pad: 0, shift: false, pressed: true });
    await emit({ kind: "pad", deck: "a", mode: "sampler", pad: 0, shift: false, pressed: false });
    await emit({ kind: "pad", deck: "a", mode: "sampler", pad: 1, shift: false, pressed: true });
    expect(warn.mock.calls).toEqual([
      ["perform: no command for pad:sampler:0:false"],
      ["perform: no command for pad:sampler:1:false"],
    ]);
    expect(runCommand).not.toHaveBeenCalled();
  });

  it("tracks the controller and reports LEDs once per change while it is there", async () => {
    expect(state.connected).toBeNull();
    expect(host.leds).toHaveLength(0);
    await emit({ kind: "connected", name: "DDJ-FLX4" });
    expect(state.connected).toBe("DDJ-FLX4");
    expect(host.leds).toHaveLength(1);
    expect(host.leds[0]?.a).toEqual({
      hotCues: Array.from({ length: 8 }, () => false), sync: false, masterTempo: false, quantize: true, cue: false, padMode: null,
    });

    await act(async () => {
      tick?.(both({ masterTempo: true }));
      tick?.(both({ masterTempo: true }));
      tick?.(both({ masterTempo: true }));
      await Promise.resolve();
    });
    expect(host.leds).toHaveLength(2);
    expect(host.leds[1]?.a).toMatchObject({ masterTempo: true, cue: true });
    expect(host.leds[1]?.b).toMatchObject({ masterTempo: false, cue: true });

    await act(async () => {
      tick?.(both({ masterTempo: true, playing: true }));
      await Promise.resolve();
    });
    expect(host.leds).toHaveLength(3);
    expect(host.leds[2]?.a.cue).toBe(false);

    await emit({ kind: "disconnected" });
    expect(state.connected).toBeNull();
    expect(tick).toBeNull();
    expect(host.leds).toHaveLength(3);
  });

  it("reads Q from the deck's saved flag", async () => {
    localStorage.setItem("rbl.deckQuantize.v1", JSON.stringify({ b: false }));
    await emit({ kind: "connected", name: "DDJ-FLX4" });
    await act(async () => {
      tick?.(both({}));
      await Promise.resolve();
    });
    expect(host.leds.at(-1)?.a.quantize).toBe(true);
    expect(host.leds.at(-1)?.b.quantize).toBe(false);
  });

  it("stops listening on unmount", () => {
    act(() => root.unmount());
    expect(host.unlisten).toHaveBeenCalledTimes(1);
    root = createRoot(document.createElement("div"));
  });
});
