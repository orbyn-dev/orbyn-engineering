import { useCallback, useState, type RefCallback } from "react";

/** The Settings designs swap the breadcrumb for a back link in a 640 px window (and a 320 px panel). */
const NARROW = 640;

/** Whether the element is narrower than that. Unmeasured (no layout) reads as wide. */
export function useNarrowPage(): [RefCallback<HTMLElement>, boolean] {
  const [narrow, setNarrow] = useState(false);
  const ref = useCallback<RefCallback<HTMLElement>>((element) => {
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
