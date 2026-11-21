/**
 * One shared clock for labels that count an age ("Checked 5 min ago"). A single interval serves every screen that
 * asks for it, ticks every 30 seconds while the page is visible, stops while it is hidden (and ticks once on return so
 * the label catches up), and is cleared when the last screen unmounts. There is never a timer per component.
 */
import { useEffect, useState } from "react";

export const CLOCK_TICK_MS = 30_000;

type Listener = (now: number) => void;
const listeners = new Set<Listener>();
let timer: ReturnType<typeof setInterval> | undefined;

const visible = () => typeof document === "undefined" || document.visibilityState !== "hidden";

function tick() {
  const now = Date.now();
  for (const listener of [...listeners]) listener(now);
}

function sync() {
  if (listeners.size > 0 && visible()) {
    timer ??= setInterval(tick, CLOCK_TICK_MS);
  } else if (timer !== undefined) {
    clearInterval(timer);
    timer = undefined;
  }
}

function onVisibility() {
  if (listeners.size > 0 && visible()) tick();
  sync();
}

function subscribe(listener: Listener): () => void {
  if (listeners.size === 0 && typeof document !== "undefined") document.addEventListener("visibilitychange", onVisibility);
  listeners.add(listener);
  sync();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && typeof document !== "undefined") document.removeEventListener("visibilitychange", onVisibility);
    sync();
  };
}

/** The current time in milliseconds, renewed every 30 s while the page is visible. */
export function useClock(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    return subscribe(setNow);
  }, []);
  return now;
}
