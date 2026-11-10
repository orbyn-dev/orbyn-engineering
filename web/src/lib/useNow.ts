import { useEffect, useState } from "react";

/** One timer per cadence, shared by every clock on the page that ticks at it. */
const tickers = new Map<number, { listeners: Set<() => void>; timer: ReturnType<typeof setInterval> }>();

function subscribe(intervalMs: number, listener: () => void): () => void {
  let ticker = tickers.get(intervalMs);
  if (!ticker) {
    const listeners = new Set<() => void>();
    ticker = {
      listeners,
      timer: setInterval(() => {
        for (const run of listeners) run();
      }, intervalMs),
    };
    tickers.set(intervalMs, ticker);
  }
  ticker.listeners.add(listener);
  const joined = ticker;
  return () => {
    joined.listeners.delete(listener);
    if (joined.listeners.size === 0) {
      clearInterval(joined.timer);
      tickers.delete(intervalMs);
    }
  };
}

/**
 * The current time, ticking only while `active`. A page full of running rows shares one
 * timer instead of starting one per clock; clocks on another cadence get their own.
 */
export function useNow(active: boolean, intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    return subscribe(intervalMs, () => setNow(Date.now()));
  }, [active, intervalMs]);
  return now;
}
