/**
 * The controller bridge: `perform:event` in, `runCommand` out, and the deck
 * state the controller's LEDs show sent back with `perform_leds`. Mounted
 * once, by `PerformBridge`.
 *
 * What the LEDs can show is limited to state that lives outside upstream's
 * components, because `Player.tsx` and `App.tsx` are not ours to edit beyond
 * a hook line (`docs/fork.md`):
 *
 * - master tempo and CUE: from `deck:tick`, as `usePlayback` reads them.
 * - quantize: `deckQuantize.ts`'s saved flag, re-read on each tick. Player
 *   saves it in an effect and announces nothing, and ticks stop when both
 *   decks fall silent, so a Q change on a stopped deck shows at the next tick.
 * - hot cue slots, SYNC and the pad mode: not reachable. The slots are the
 *   `cues` Player is handed, SYNC is `App.tsx`'s `synced` state and the pad
 *   mode is Player's `padMode`; all three are component state. They are sent
 *   dark (`padMode: null`) until a hook line publishes them.
 */
import { useEffect, useRef, useState } from "react";

import { getBackend } from "@/ipc/client";
import { onPerformEvent, performLeds, performStatus } from "@/ipc/perform";
import type { DeckId, DeckTick, Tick } from "@/ipc/types";
import { runCommand } from "@/lib/commands";
import { loadDeckQuantize } from "@/lib/deckQuantize";
import { commandsFor, isPress } from "./mapping";
import type { PerformDeckLeds, PerformEvent, PerformLeds } from "./types";

export interface PerformBridgeState {
  /** The controller's port name while one is connected. */
  connected: string | null;
}

const UNREACHABLE_HOT_CUES: readonly boolean[] = Array.from({ length: 8 }, () => false);

function darkDeck(deck: DeckId): PerformDeckLeds {
  return {
    hotCues: [...UNREACHABLE_HOT_CUES],
    sync: false,
    masterTempo: false,
    quantize: loadDeckQuantize(deck),
    cue: false,
    padMode: null,
  };
}

function deckLeds(deck: DeckId, tick: DeckTick): PerformDeckLeds {
  return {
    hotCues: [...UNREACHABLE_HOT_CUES],
    sync: false,
    masterTempo: tick.masterTempo,
    quantize: loadDeckQuantize(deck),
    // [ASSUME] lit while a loaded deck is stopped, as a CDJ's CUE is when it
    // waits on its cue point; the host may blink it, which is its business.
    cue: tick.loaded && !tick.playing,
    padMode: null,
  };
}

function sameDeck(a: PerformDeckLeds, b: PerformDeckLeds): boolean {
  return a.sync === b.sync
    && a.masterTempo === b.masterTempo
    && a.quantize === b.quantize
    && a.cue === b.cue
    && a.padMode === b.padMode
    && a.hotCues.length === b.hotCues.length
    && a.hotCues.every((set, index) => set === b.hotCues[index]);
}

/** The same object when nothing changed, so a tick that changes no LED renders nothing. */
function nextLeds(previous: PerformLeds, tick: Tick): PerformLeds {
  const next = { a: deckLeds("a", tick.a), b: deckLeds("b", tick.b) };
  return sameDeck(previous.a, next.a) && sameDeck(previous.b, next.b) ? previous : next;
}

/** An event with its deck, amount and press left out: one warning covers all of them. */
function shapeOf(event: PerformEvent): string {
  switch (event.kind) {
    case "connected":
    case "disconnected":
      return event.kind;
    case "button":
      return `button:${event.button}:${event.shift}`;
    case "pad":
      return `pad:${event.mode}:${event.pad}:${event.shift}`;
    case "browse":
      return `browse:${Math.sign(event.delta)}:${event.shift}`;
    case "global":
      return `global:${event.button}:${event.shift}`;
  }
}

/**
 * Runs the commands a controller event maps to. A press that maps to
 * nothing is said once per shape, so a held-down habit does not flood the
 * console. A command nothing answers (an empty deck, no list on screen) is
 * the same quiet no-op it is for a key.
 */
export function routePerformEvent(event: PerformEvent, warned: Set<string>): void {
  const commands = commandsFor(event);
  if (commands.length === 0) {
    if (!isPress(event)) return;
    const shape = shapeOf(event);
    if (warned.has(shape)) return;
    warned.add(shape);
    console.warn(`perform: no command for ${shape}`);
    return;
  }
  for (const { action, scope } of commands) runCommand(action, { scope });
}

export function usePerformBridge(): PerformBridgeState {
  const [connected, setConnected] = useState<string | null>(null);
  const [leds, setLeds] = useState<PerformLeds>(() => ({ a: darkDeck("a"), b: darkDeck("b") }));
  const warned = useRef(new Set<string>());

  // The event stream, and the controller that was already there when the
  // window opened (its `connected` event went out before anyone listened).
  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void performStatus()
      .then((status) => {
        if (live) setConnected(status.connected);
      })
      .catch((error: unknown) => console.error("perform: status failed", error));
    void onPerformEvent((event) => {
      if (event.kind === "connected") setConnected(event.name);
      else if (event.kind === "disconnected") setConnected(null);
      else routePerformEvent(event, warned.current);
    })
      .then((unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      })
      .catch((error: unknown) => console.error("perform: could not listen for controller events", error));
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  // Deck state, only while a controller is there to show it.
  const linked = connected !== null;
  useEffect(() => {
    if (!linked) return;
    let live = true;
    let stop: (() => void) | undefined;
    void getBackend()
      .then((backend) => {
        if (!live) return;
        stop = backend.onDeckTick((tick) => setLeds((previous) => nextLeds(previous, tick)));
      })
      .catch((error: unknown) => console.error("perform: no deck ticks for the LEDs", error));
    return () => {
      live = false;
      stop?.();
    };
  }, [linked]);

  // One report per change: `leds` keeps its identity across ticks that
  // change nothing. A new controller gets the full state.
  useEffect(() => {
    if (connected === null) return;
    performLeds(leds).catch((error: unknown) => console.error(`perform: LED report to ${connected} failed`, error));
  }, [connected, leds]);

  return { connected };
}
