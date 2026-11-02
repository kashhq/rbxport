/**
 * @vitest-environment jsdom
 *
 * What Player's and App's hook lines publish: the pads' slots by letter, the
 * pad row's mode, and SYNC, with a deck that goes away going dark.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { Cue } from "@/ipc/types";
import type { PadMode } from "@/lib/player";
import {
  __resetPerformDeckState, hotCueSlots, performDeckState, performPadMode, usePublishPerformDeck, usePublishPerformSync,
} from "./deckState";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const cue = (letter: string, memory = false): Cue => ({ id: letter, positionMs: 0, outMs: 0, letter, memory, colour: null });

let root: Root;

function Deck({ cues, padMode }: { cues: readonly Cue[]; padMode: PadMode }) {
  usePublishPerformDeck("b", { cues, padMode });
  return null;
}

function Shell({ a, b }: { a: boolean; b: boolean }) {
  usePublishPerformSync({ a, b });
  return null;
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  __resetPerformDeckState();
  root = createRoot(document.createElement("div"));
});

afterEach(() => {
  act(() => root.unmount());
});

describe("deckState", () => {
  it("marks slots A to H by letter, ignoring memory cues and letters past H", () => {
    expect(hotCueSlots([cue("B"), cue("H"), cue("I"), cue("", true)])).toEqual([false, true, false, false, false, false, false, true]);
  });

  it("maps the pad row's modes", () => {
    expect(performPadMode("cue")).toBe("hotCue");
    expect(performPadMode("grid")).toBeNull();
  });

  it("publishes a deck's pads while it is mounted and darkens them after", () => {
    act(() => root.render(<Deck cues={[cue("A"), cue("C")]} padMode="cue" />));
    expect(performDeckState().b).toEqual({
      hotCues: [true, false, true, false, false, false, false, false], synced: false, padMode: "hotCue",
    });
    expect(performDeckState().a.padMode).toBeNull();
    act(() => root.render(<Deck cues={[cue("A"), cue("C")]} padMode="grid" />));
    expect(performDeckState().b.padMode).toBeNull();
    act(() => root.unmount());
    expect(performDeckState().b.hotCues.some(Boolean)).toBe(false);
    root = createRoot(document.createElement("div"));
  });

  it("publishes SYNC for both decks and keeps identity when nothing changes", () => {
    act(() => root.render(<Shell a b={false} />));
    const first = performDeckState();
    expect([first.a.synced, first.b.synced]).toEqual([true, false]);
    act(() => root.render(<Shell a b={false} />));
    expect(performDeckState()).toBe(first);
  });
});
