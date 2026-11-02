/**
 * The controller host's IPC: `perform_status`, `perform_leds` and the
 * `perform:event` stream (see `src/perform/types.ts` for the contract).
 *
 * Kept out of the `Backend` interface on purpose: this is the fork's surface
 * (`docs/fork.md`), and adding it to `Backend` would mean editing upstream's
 * types, client and mock together. Outside Tauri there is no controller, so
 * every call is a no-op that resolves at once.
 */
import type { PerformEvent, PerformLeds, PerformStatus } from "@/perform/types";
import { tauriInvoke } from "./invoke";

// The same test `client.ts` makes: the mock backend runs exactly when this is false.
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const NO_CONTROLLER: PerformStatus = { connected: null };

export async function performStatus(): Promise<PerformStatus> {
  if (!isTauri) return NO_CONTROLLER;
  const invoke = await tauriInvoke();
  return invoke<PerformStatus>("perform_status");
}

export async function performLeds(leds: PerformLeds): Promise<void> {
  if (!isTauri) return;
  const invoke = await tauriInvoke();
  await invoke<void>("perform_leds", { leds });
}

/** Resolves to the unsubscribe once the listener is in place. */
export async function onPerformEvent(listener: (event: PerformEvent) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<PerformEvent>("perform:event", (event) => listener(event.payload));
}
