import { useEffect } from "react";
import { useUi } from "../state/ui";
import { useSettledAfter } from "../../lib/lingering";
import { MOTION } from "../../lib/motion";
import { usePrefersReducedMotion } from "../../lib/useLayout";

export const MAX_PANEL_ANIMATION_MS = 400;

/**
 * `enabled` is full motion, which the composer's travel uses and reduced motion turns off.
 * `panels` stays on under reduced motion, where the stylesheet swaps the slide for a fade.
 * `exitMs` is how long a closing pane stays mounted to finish leaving.
 */
export function usePanelMotion(): { enabled: boolean; panels: boolean; durationMs: number; exitMs: number } {
  const durationMs = useUi((state) => state.panelAnimationMs);
  const reduced = usePrefersReducedMotion();
  const panels = durationMs > 0;
  return {
    enabled: panels && !reduced,
    panels,
    durationMs,
    exitMs: !panels ? 0 : reduced ? MOTION.fast : MOTION.slowOut,
  };
}

/**
 * Publishes the setting on <html> for the stylesheet. It goes quiet across a
 * route change for two frames, so a pane mounting with the new route stays still.
 */
export function usePanelMotionRoot(routeKey: string): void {
  const { panels, durationMs } = usePanelMotion();
  const settled = useSettledAfter(routeKey);

  const on = panels && settled;
  useEffect(() => {
    const { dataset, style } = document.documentElement;
    dataset.panelMotion = on ? "on" : "off";
    // The default is the --panel-duration token; only a different setting overrides it.
    if (durationMs === MOTION.slow) style.removeProperty("--panel-duration");
    else style.setProperty("--panel-duration", `${durationMs}ms`);
  }, [on, durationMs]);
}
