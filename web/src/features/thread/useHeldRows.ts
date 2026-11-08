import { useEffect, useState, type RefObject } from "react";
import { MAX_HELD_BETWEEN } from "./windowing";

const rowKey = (node: Node | null): string | undefined =>
  (node instanceof Element ? node : node?.parentElement)?.closest<HTMLElement>("[data-entry]")?.dataset.entry;

/**
 * Rows that must stay mounted wherever the view is: the one with focus, and the ones a text
 * selection starts and ends in, with the rows between them when that is a modest run. A
 * longer selection keeps its two ends but only what is mounted can be copied.
 */
export function useHeldRows(scroller: RefObject<HTMLElement | null>, indexByKey: ReadonlyMap<string, number>, keys: readonly string[]) {
  const [held, setHeld] = useState<readonly string[]>([]);
  useEffect(() => {
    const root = scroller.current;
    if (!root) return;
    const update = () => {
      const next = new Set<string>();
      const focused = document.activeElement;
      if (focused && root.contains(focused)) {
        const key = rowKey(focused);
        if (key) next.add(key);
      }
      const selection = document.getSelection();
      if (selection && !selection.isCollapsed) {
        const from = rowKey(selection.anchorNode);
        const to = rowKey(selection.focusNode);
        if (from) next.add(from);
        if (to) next.add(to);
        const a = from === undefined ? undefined : indexByKey.get(from);
        const b = to === undefined ? undefined : indexByKey.get(to);
        if (a !== undefined && b !== undefined && Math.abs(a - b) <= MAX_HELD_BETWEEN) {
          for (let at = Math.min(a, b); at <= Math.max(a, b); at++) {
            const key = keys[at];
            if (key) next.add(key);
          }
        }
      }
      setHeld((prev) => (prev.length === next.size && prev.every((key) => next.has(key)) ? prev : [...next]));
    };
    document.addEventListener("selectionchange", update);
    root.addEventListener("focus", update, true);
    root.addEventListener("blur", update, true);
    return () => {
      document.removeEventListener("selectionchange", update);
      root.removeEventListener("focus", update, true);
      root.removeEventListener("blur", update, true);
    };
  }, [scroller, indexByKey, keys]);
  return held;
}
