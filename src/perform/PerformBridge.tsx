import { usePerformBridge } from "./usePerformBridge";

/**
 * The controller bridge, as something `App.tsx` can mount with one line.
 * Draws nothing: performance mode's controls are the player view's own.
 */
export function PerformBridge(): null {
  usePerformBridge();
  return null;
}
