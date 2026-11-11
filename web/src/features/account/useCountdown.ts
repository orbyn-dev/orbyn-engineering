import { useEffect, useState } from "react";
import { useNow } from "../../lib/useNow";

/** A wait the service asked for: how long, counted from when the answer arrived. */
export interface Wait {
  seconds: number;
  at: number;
}

/** Seconds left of `wait`, ticking once a second while it runs. Null when there is none. */
export function useCountdown(wait: Wait | null): number | null {
  const until = wait ? wait.at + wait.seconds * 1000 : null;
  const now = useNow(until !== null && Date.now() < until, 1000);
  if (until === null) return null;
  return Math.max(0, Math.ceil((until - Math.max(now, wait ? wait.at : 0)) / 1000));
}

/** True once `busy` has lasted `ms`: a spinner that only shows when the wait is a visible one. */
export function useAfter(busy: boolean, ms = 400): boolean {
  const [late, setLate] = useState(false);
  useEffect(() => {
    if (!busy) {
      setLate(false);
      return;
    }
    const timer = setTimeout(() => setLate(true), ms);
    return () => clearTimeout(timer);
  }, [busy, ms]);
  return late;
}
