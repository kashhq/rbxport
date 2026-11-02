/**
 * The deck state the controller's LEDs show that lives inside upstream's
 * components: hot cue slots and the pad row's mode (Player.tsx), and BEAT
 * SYNC (App.tsx). Each owner publishes it with one `FLX:` hook line; the
 * bridge reads it with `usePerformDeckState`.
 *
 * Used by: `Player.tsx` (`usePublishPerformDeck`), `App.tsx`
 * (`usePublishPerformSync`), `usePerformBridge.ts`.
 */
import { useEffect, useSyncExternalStore } from "react";

import type { Cue, DeckId } from "@/ipc/types";
import { hotCue } from "@/lib/cues";
import type { PadMode } from "@/lib/player";
import type { PerformDeckLeds } from "./types";

export interface PerformDeckState {
  /** Slot A..H set; always eight long. */
  hotCues: readonly boolean[];
  synced: boolean;
  padMode: PerformDeckLeds["padMode"];
}

export type PerformDecks = Readonly<Record<DeckId, PerformDeckState>>;

const HOT_CUE_LETTERS = ["A", "B", "C", "D", "E", "F", "G", "H"] as const;
const NO_HOT_CUES: readonly boolean[] = HOT_CUE_LETTERS.map(() => false);
const EMPTY: PerformDeckState = { hotCues: NO_HOT_CUES, synced: false, padMode: null };

let decks: PerformDecks = { a: EMPTY, b: EMPTY };
const listeners = new Set<() => void>();

function sameState(a: PerformDeckState, b: PerformDeckState): boolean {
  return a.synced === b.synced
    && a.padMode === b.padMode
    && a.hotCues.length === b.hotCues.length
    && a.hotCues.every((set, index) => set === b.hotCues[index]);
}

/** Merges what one owner knows about a deck; a no-op when nothing changed. */
export function publishPerformDeckState(deck: DeckId, partial: Partial<PerformDeckState>): void {
  const next = { ...decks[deck], ...partial };
  if (sameState(decks[deck], next)) return;
  decks = { ...decks, [deck]: next };
  for (const listener of listeners) listener();
}

export function subscribePerformDeckState(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Both decks; the object only changes identity when a value does. */
export function performDeckState(): PerformDecks {
  return decks;
}

export function usePerformDeckState(): PerformDecks {
  return useSyncExternalStore(subscribePerformDeckState, performDeckState);
}

/** Which hot cue slots A..H a track's cues fill, as the pads draw them. */
export function hotCueSlots(cues: readonly Cue[]): boolean[] {
  return HOT_CUE_LETTERS.map((letter) => hotCue(cues, letter) !== null);
}

/**
 * Player's pad row shows either the hot cues or GRID EDIT; it has no beat
 * jump, beat loop or key shift page, so those never come from the UI.
 */
export function performPadMode(mode: PadMode): PerformDeckLeds["padMode"] {
  return mode === "cue" ? "hotCue" : null;
}

/**
 * Player's hook line. A deck that goes away (FULL BROWSER, 1 PLAYER) takes
 * its pads with it, so its slots and mode go dark until it is back.
 */
export function usePublishPerformDeck(deck: DeckId, { cues, padMode }: { cues: readonly Cue[]; padMode: PadMode }): void {
  // A cue list refetch that fills the same slots is dropped by the store.
  useEffect(() => {
    publishPerformDeckState(deck, { hotCues: hotCueSlots(cues), padMode: performPadMode(padMode) });
  }, [deck, cues, padMode]);
  useEffect(() => () => publishPerformDeckState(deck, { hotCues: NO_HOT_CUES, padMode: null }), [deck]);
}

/** App.tsx's hook line: BEAT SYNC is the shell's state, one flag per deck. */
export function usePublishPerformSync(synced: Readonly<Record<DeckId, boolean>>): void {
  useEffect(() => {
    publishPerformDeckState("a", { synced: synced.a });
    publishPerformDeckState("b", { synced: synced.b });
  }, [synced.a, synced.b]);
}

/** Test seam: back to two empty decks. */
export function __resetPerformDeckState(): void {
  decks = { a: EMPTY, b: EMPTY };
}
