/**
 * The controller bridge: `perform:event` in, `runCommand` out, and the deck
 * state the controller's LEDs show sent back with `perform_leds`. Mounted
 * once, by `PerformBridge`.
 *
 * Where each LED's state comes from:
 *
 * - CUE: `deck:tick`, as `usePlayback` reads it.
 * - hot cue slots, pad mode and SYNC: `deckState.ts`, which Player and App
 *   publish to from their `FLX:` hook lines.
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { getBackend } from "@/ipc/client";
import { onPerformEvent, performLeds, performStatus } from "@/ipc/perform";
import type { DeckId, DeckTick, Tick } from "@/ipc/types";
import { runCommand } from "@/lib/commands";
import { usePerformDeckState, type PerformDeckState } from "./deckState";
import { commandsFor, isPress } from "./mapping";
import type { PerformDeckLeds, PerformEvent, PerformLeds } from "./types";

export interface PerformBridgeState {
  /** The controller's port name while one is connected. */
  connected: string | null;
}

/** The LEDs the engine's tick decides. */
type EngineLeds = Pick<PerformDeckLeds, "cue">;
type EngineDecks = Readonly<Record<DeckId, EngineLeds>>;

function engineLeds(tick: DeckTick | null): EngineLeds {
  return {
    // [ASSUME] lit while a loaded deck is stopped, as a CDJ's CUE is when it
    // waits on its cue point; the host may blink it, which is its business.
    cue: tick !== null && tick.loaded && !tick.playing,
  };
}

function sameEngine(a: EngineLeds, b: EngineLeds): boolean {
  return a.cue === b.cue;
}

/** The same object when nothing changed, so a tick that changes no LED renders nothing. */
function nextEngine(previous: EngineDecks, tick: Tick): EngineDecks {
  const next = { a: engineLeds(tick.a), b: engineLeds(tick.b) };
  return sameEngine(previous.a, next.a) && sameEngine(previous.b, next.b) ? previous : next;
}

function deckLeds(ui: PerformDeckState, engine: EngineLeds): PerformDeckLeds {
  return { hotCues: [...ui.hotCues], sync: ui.synced, padMode: ui.padMode, ...engine };
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
  const [engine, setEngine] = useState<EngineDecks>(() => ({ a: engineLeds(null), b: engineLeds(null) }));
  const ui = usePerformDeckState();
  // Changes identity only when an LED does: both inputs keep theirs otherwise.
  const leds = useMemo<PerformLeds>(() => ({ a: deckLeds(ui.a, engine.a), b: deckLeds(ui.b, engine.b) }), [ui, engine]);
  const warned = useRef(new Set<string>());

  // The event stream, and the controller that was already there when the
  // window opened (its `connected` event went out before anyone listened).
  useEffect(() => {
    let live = true;
    // Once any event has arrived it is newer than whatever the status call
    // answers, however late that answer lands.
    let heard = false;
    let stop: (() => void) | undefined;
    void performStatus()
      .then((status) => {
        if (live && !heard) setConnected(status.connected);
      })
      .catch((error: unknown) => console.error("perform: status failed", error));
    void onPerformEvent((event) => {
      heard = true;
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

  // Engine state, only while a controller is there to show it.
  const linked = connected !== null;
  useEffect(() => {
    if (!linked) return;
    let live = true;
    let stop: (() => void) | undefined;
    void getBackend()
      .then((backend) => {
        if (!live) return;
        stop = backend.onDeckTick((tick) => setEngine((previous) => nextEngine(previous, tick)));
      })
      .catch((error: unknown) => console.error("perform: no deck ticks for the LEDs", error));
    return () => {
      live = false;
      stop?.();
    };
  }, [linked]);

  // One report per change, never per tick. A new controller gets the full state.
  useEffect(() => {
    if (connected === null) return;
    performLeds(leds).catch((error: unknown) => console.error(`perform: LED report to ${connected} failed`, error));
  }, [connected, leds]);

  return { connected };
}
