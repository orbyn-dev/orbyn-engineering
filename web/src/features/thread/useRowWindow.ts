import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type RefObject } from "react";
import { flushSync } from "react-dom";
import type { TimelineEntry } from "./timeline";
import { buildOffsets, indexAt, planRows, sizesFor, totalHeight, visibleRange, type Segment } from "./windowing";

/** A row near the top of the view and how far its top sits from the view's top. */
export interface Anchor {
  key: string;
  offset: number;
}

/** The padding above the first row, until a mounted row tells us the real number. */
const FIRST_CONTENT_TOP = 24;
/** Scrolling less than this does not change which rows are mounted (the overscan is far larger). */
const QUANTUM = 100;
/** How long a scroll target keeps being re-aligned while rows around it settle. */
const SETTLE_MS = 400;

type Align = "start" | "center";
interface Goal {
  key: string;
  align: Align;
  resolve(element: HTMLElement | null): void;
}

/** The row's block as the layout sees it: a "new since" line above a row counts as part of it. */
function unit(element: HTMLElement): { top: number; height: number } {
  const rect = element.getBoundingClientRect();
  const before = element.previousElementSibling;
  if (before instanceof HTMLElement && before.classList.contains("visit-mark")) {
    const mark = before.getBoundingClientRect();
    const margin = Number.parseFloat(getComputedStyle(before).marginTop) || 0;
    return { top: mark.top - margin, height: rect.bottom - mark.top + margin };
  }
  return { top: rect.top, height: rect.height };
}

/**
 * Mounts only the rows near the view. Rows are positioned by flow: each run of unmounted rows
 * is one spacer sized from measured heights (or an estimate), so the scroll height and the
 * scrollbar behave as if everything were there. Measured heights replace estimates as rows
 * mount or resize, and the row at the top of the view stays where it is while that happens.
 */
export function useRowWindow({
  threadId,
  entries,
  scroller,
  pinned,
  following,
  indexByKey,
  held,
  restore,
  onUnpin,
}: {
  threadId: string;
  entries: readonly TimelineEntry[];
  scroller: RefObject<HTMLDivElement | null>;
  /** True while the view follows the end of the thread. */
  pinned: RefObject<boolean>;
  following: boolean;
  /** Position of each row by key. */
  indexByKey: ReadonlyMap<string, number>;
  /** Row keys that must stay mounted: the focused row, a selection, a scroll target. */
  held: readonly string[];
  /** Where to open when the thread was left scrolled up. */
  restore: Anchor | null;
  onUnpin(): void;
}) {
  const [sizes] = useState(() => sizesFor(threadId));
  const [rev, bump] = useReducer((n: number) => n + 1, 0);
  const offsets = useMemo(() => buildOffsets(entries, sizes), [entries, sizes, rev]); // eslint-disable-line react-hooks/exhaustive-deps
  const [target, setTarget] = useState<string | null>(null);

  const [view, setView] = useState(() => {
    const at = restore ? indexByKey.get(restore.key) : undefined;
    return {
      top: at === undefined ? 0 : Math.max(0, offsets[at]! - restore!.offset),
      height: typeof window === "undefined" ? 800 : window.innerHeight,
    };
  });

  const top = following ? Math.max(0, totalHeight(offsets) - view.height) : view.top;
  const range = useMemo(() => visibleRange(offsets, top, view.height), [offsets, top, view.height]);
  const heldRows = useMemo(() => {
    const rows = held.map((key) => indexByKey.get(key)).filter((at): at is number => at !== undefined);
    const goal = target === null ? undefined : indexByKey.get(target);
    return goal === undefined ? rows : [...rows, goal];
  }, [held, indexByKey, target]);
  const segments: readonly Segment[] = useMemo(() => planRows(offsets, range, heldRows), [offsets, range, heldRows]);

  const latest = useRef({ offsets, indexByKey, entries });
  const contentTop = useRef(FIRST_CONTENT_TOP);
  const anchor = useRef<Anchor | null>(restore);
  const goal = useRef<Goal | null>(null);
  const mounted = useRef(new Map<string, HTMLElement>());

  const capture = useCallback(() => {
    const element = scroller.current;
    const { offsets: now, entries: rows } = latest.current;
    if (!element || rows.length === 0) {
      anchor.current = null;
      return;
    }
    const rel = element.scrollTop - contentTop.current;
    const at = indexAt(now, rel);
    anchor.current = { key: rows[at]!.key, offset: now[at]! - rel };
  }, [scroller]);

  const measure = useCallback((): boolean => {
    let changed = false;
    for (const [key, element] of mounted.current) {
      if (sizes.set(key, unit(element).height)) changed = true;
    }
    return changed;
  }, [sizes]);

  const onScroll = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    capture();
    const rel = element.scrollTop - contentTop.current;
    const height = element.clientHeight;
    setView((prev) => (Math.abs(prev.top - rel) < QUANTUM && prev.height === height ? prev : { top: rel, height }));
  }, [capture, scroller]);

  // Rows can change height after they mount: images, code, streaming text, a card opening.
  const onRowsResize = useRef<() => void>(() => {});
  const [observer] = useState(() =>
    typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => onRowsResize.current()),
  );
  const rowRef = useCallback(
    (element: HTMLElement | null) => {
      const key = element?.dataset.entry;
      if (!element || !key) return;
      mounted.current.set(key, element);
      observer?.observe(element);
      return () => {
        if (mounted.current.get(key) === element) mounted.current.delete(key);
        observer?.unobserve(element);
      };
    },
    [observer],
  );

  useLayoutEffect(() => {
    latest.current = { offsets, indexByKey, entries };
    onRowsResize.current = () => {
      if (!measure()) return;
      flushSync(bump);
    };
  });

  useEffect(() => {
    const element = scroller.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(() => onScroll());
    watch.observe(element);
    return () => watch.disconnect();
  }, [scroller, onScroll]);
  useEffect(() => () => observer?.disconnect(), [observer]);

  // After every commit: take real heights, then hold the place the person was looking at.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    if (measure()) {
      bump();
      return;
    }
    const first = segments.find((segment) => segment.kind === "row");
    const firstElement = first && mounted.current.get(entries[first.index]!.key);
    if (first && firstElement) {
      const place = unit(firstElement).top - element.getBoundingClientRect().top + element.scrollTop - offsets[first.index]!;
      if (Math.abs(place - contentTop.current) > 0.5) contentTop.current = place;
    }
    const wanted = goal.current;
    const rowAt = wanted ? indexByKey.get(wanted.key) : undefined;
    if (wanted && rowAt !== undefined) {
      const row = mounted.current.get(wanted.key);
      if (row) {
        if (typeof row.scrollIntoView === "function") row.scrollIntoView({ block: wanted.align });
        wanted.resolve(row);
      } else {
        const lead = wanted.align === "center" ? element.clientHeight / 2 : 0;
        element.scrollTop = Math.max(0, offsets[rowAt]! + contentTop.current - lead);
      }
    } else if (pinned.current) {
      element.scrollTop = element.scrollHeight;
    } else if (anchor.current) {
      const at = indexByKey.get(anchor.current.key);
      if (at !== undefined) {
        const want = offsets[at]! + contentTop.current - anchor.current.offset;
        if (Math.abs(want - element.scrollTop) > 0.5) element.scrollTop = want;
      }
    }
    capture();
    const rel = element.scrollTop - contentTop.current;
    setView((prev) => (Math.abs(prev.top - rel) < QUANTUM && prev.height === element.clientHeight ? prev : { top: rel, height: element.clientHeight }));
  }, [scroller, measure, segments, entries, indexByKey, pinned, following, capture, offsets]);

  const scrollToIndex = useCallback(
    (index: number, align: Align): Promise<HTMLElement | null> => {
      const { entries: rows, offsets: now } = latest.current;
      const row = rows[index];
      const element = scroller.current;
      if (!row || !element) return Promise.resolve(null);
      onUnpin();
      return new Promise((resolve) => {
        goal.current?.resolve(null);
        const finish = (found: HTMLElement | null) => {
          resolve(found);
        };
        goal.current = { key: row.key, align, resolve: finish };
        setTarget(row.key);
        const lead = align === "center" ? element.clientHeight / 2 : 0;
        element.scrollTop = Math.max(0, now[index]! + contentTop.current - lead);
        setTimeout(() => {
          if (goal.current?.resolve === finish) goal.current = null;
          finish(mounted.current.get(row.key) ?? null);
          setTarget((current) => (current === row.key ? null : current));
        }, SETTLE_MS);
      });
    },
    [onUnpin, scroller],
  );

  const offsetOfRow = useCallback((index: number): number | null => {
    const { offsets: now } = latest.current;
    return index >= 0 && index < now.length - 1 ? now[index]! + contentTop.current : null;
  }, []);

  return { segments, rowRef, onScroll, scrollToIndex, offsetOfRow, anchor: () => anchor.current, rev };
}
