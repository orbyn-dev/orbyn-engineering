import { useEffect, useState } from "react";

/**
 * True two frames after `key` becomes non-null, and false again when it changes. A pane
 * that was already there when its page loaded stays still; only later opens and closes move.
 */
export function useSettledAfter(key: string | null): boolean {
  const [settled, setSettled] = useState<string | null>(null);
  useEffect(() => {
    if (key === null) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => setSettled(key));
    });
    return () => cancelAnimationFrame(frame);
  }, [key]);
  return key !== null && settled === key;
}

/**
 * Stays true for `exitMs` after `open` drops, so a closing pane can animate out. The change
 * is taken while rendering, so the pane never unmounts for a frame between closing and lingering.
 */
export function useLingering(open: boolean, exitMs: number): boolean {
  const [lingering, setLingering] = useState(false);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    setLingering(!open && exitMs > 0);
  }

  useEffect(() => {
    if (!lingering) return;
    const timer = setTimeout(() => setLingering(false), exitMs);
    return () => clearTimeout(timer);
  }, [lingering, exitMs]);

  return open || lingering;
}
