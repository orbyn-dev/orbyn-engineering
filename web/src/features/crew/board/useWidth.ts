import { useEffect, useRef, useState } from "react";

/** Below this width the board is a list of state groups, not columns (Crew Designs: below 640 px). */
const NARROW = 640;

/** Whether the element is narrower than a window the board needs for its columns. Unmeasured (no layout) reads as wide. */
export function useNarrow(): [React.RefObject<HTMLElement | null>, boolean] {
  const ref = useRef<HTMLElement>(null);
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? 0;
      if (width > 0) setNarrow(width < NARROW);
    });
    watch.observe(element);
    return () => watch.disconnect();
  }, []);
  return [ref, narrow];
}
