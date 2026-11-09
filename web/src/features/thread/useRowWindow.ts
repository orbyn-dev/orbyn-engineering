import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type RefObject } from "react";
import type { TimelineEntry } from "../../product/timeline/timeline";
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
/** The longest a scroll target waits for its row to mount before giving up. */
const GOAL_MS = 400;

type Align = "start" | "center";
interface Goal {
  key: string;
  align: Align;
  /** The row's height at the last alignment; a second pass at the same height ends the goal. */
  height: number | null;
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
    const goalRow = target === null ? undefined : indexByKey.get(target);
    return goalRow === undefined ? rows : [...rows, goalRow];
  }, [held, indexByKey, target]);
  const segments: readonly Segment[] = useMemo(() => planRows(offsets, range, heldRows), [offsets, range, heldRows]);

  const latest = useRef({ offsets, indexByKey, entries });
  const contentTop = useRef(FIRST_CONTENT_TOP);
  const anchor = useRef<Anchor | null>(restore);
  const goal = useRef<Goal | null>(null);
  const goalTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const laidOut = useRef<Float64Array | null>(null);
  const mounted = useRef(new Map<string, HTMLElement>());
  /** Rows that mounted since the last commit and have not been measured yet. */
  const fresh = useRef(new Set<string>());
  /** A height change is waiting for its commit; the anchor must not be re-read before it lands. */
  const relayout = useRef(false);

  const stickToEnd = useCallback(() => {
    const element = scroller.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [scroller]);

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

  /** Measures the rows that just mounted; rows already in the page report their own changes. */
  const measureFresh = useCallback((): boolean => {
    let changed = false;
    for (const key of fresh.current) {
      const element = mounted.current.get(key);
      if (element && sizes.set(key, unit(element).height)) changed = true;
    }
    fresh.current.clear();
    return changed;
  }, [sizes]);

  const onScroll = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    if (!relayout.current) capture();
    const rel = element.scrollTop - contentTop.current;
    const height = element.clientHeight;
    setView((prev) => (Math.abs(prev.top - rel) < QUANTUM && prev.height === height ? prev : { top: rel, height }));
  }, [capture, scroller]);

  // Rows can change height after they mount: images, code, streaming text, a card opening. A row
  // above the reading place moves it, so the view is shifted by the same amount at once, before
  // the next paint, and the new heights follow through a normal render.
  const onRowsResize = useRef<(changed: readonly ResizeObserverEntry[]) => void>(() => {});
  const [observer] = useState(() =>
    typeof ResizeObserver === "undefined" ? null : new ResizeObserver((changed) => onRowsResize.current(changed)),
  );
  const rowRef = useCallback(
    (element: HTMLElement | null) => {
      const key = element?.dataset.entry;
      if (!element || !key) return;
      mounted.current.set(key, element);
      fresh.current.add(key);
      observer?.observe(element);
      return () => {
        if (mounted.current.get(key) === element) mounted.current.delete(key);
        observer?.unobserve(element);
      };
    },
    [observer],
  );

  const endGoal = useCallback(() => {
    clearTimeout(goalTimer.current);
    goal.current = null;
    setTarget(null);
  }, []);

  useLayoutEffect(() => {
    latest.current = { offsets, indexByKey, entries };
    onRowsResize.current = (changed) => {
      const element = scroller.current;
      const now = latest.current;
      const anchorAt = anchor.current ? now.indexByKey.get(anchor.current.key) : undefined;
      let shift = 0;
      let any = false;
      for (const entry of changed) {
        const row = entry.target as HTMLElement;
        const key = row.dataset.entry;
        if (!key) continue;
        const before = sizes.get(key);
        const next = unit(row).height;
        if (!sizes.set(key, next)) continue;
        any = true;
        const at = now.indexByKey.get(key);
        if (before !== undefined && at !== undefined && anchorAt !== undefined && at < anchorAt) shift += next - before;
      }
      if (!any) return;
      relayout.current = true;
      if (element && shift !== 0 && !pinned.current && !goal.current) element.scrollTop += shift;
      bump();
    };
  });

  useEffect(() => {
    const element = scroller.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    let width = element.clientWidth;
    const watch = new ResizeObserver(() => {
      // A new width rewraps text, so heights kept for rows out of the page are stale: forget them
      // and let the rows in the page report theirs.
      if (width > 0 && Math.abs(element.clientWidth - width) > 1) {
        sizes.retain(mounted.current.keys());
        sizes.width = element.clientWidth;
        relayout.current = true;
        bump();
      }
      width = element.clientWidth;
      onScroll();
    });
    watch.observe(element);
    return () => watch.disconnect();
  }, [scroller, onScroll, sizes]);
  useEffect(() => () => observer?.disconnect(), [observer]);
  useEffect(() => () => clearTimeout(goalTimer.current), []);

  // A scroll target gives way to the person at once.
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const yield_ = () => {
      const wanted = goal.current;
      if (!wanted) return;
      wanted.resolve(mounted.current.get(wanted.key) ?? null);
      endGoal();
    };
    const events = ["wheel", "touchstart", "keydown", "pointerdown"] as const;
    for (const name of events) element.addEventListener(name, yield_, { passive: true });
    return () => {
      for (const name of events) element.removeEventListener(name, yield_);
    };
  }, [scroller, endGoal]);

  // The first time rows are in the page, a width that differs from the one the cached heights
  // were taken at means they are stale.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    if (sizes.width > 0 && Math.abs(element.clientWidth - sizes.width) > 1) {
      sizes.retain([]);
      bump();
    }
    sizes.width = element.clientWidth;
  }, [scroller, sizes]);

  // After a commit that changed what is laid out: take heights for new rows, then hold the place.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    if (measureFresh()) {
      bump();
      return;
    }
    relayout.current = false;
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
        // Align, and once the row's height is the same on a second pass the goal is done: after
        // that the reading place is kept by the anchor and nothing pulls the view back.
        const height = unit(row).height;
        if (typeof row.scrollIntoView === "function") row.scrollIntoView({ block: wanted.align });
        wanted.resolve(row);
        if (wanted.height === height) endGoal();
        else {
          wanted.height = height;
          bump();
        }
      } else {
        const lead = wanted.align === "center" ? element.clientHeight / 2 : 0;
        element.scrollTop = Math.max(0, offsets[rowAt]! + contentTop.current - lead);
      }
    } else if (pinned.current) {
      // Only when the layout changed: a focus or selection change must never move the view.
      if (laidOut.current !== offsets) stickToEnd();
    } else if (anchor.current) {
      const at = indexByKey.get(anchor.current.key);
      if (at !== undefined) {
        const want = offsets[at]! + contentTop.current - anchor.current.offset;
        if (Math.abs(want - element.scrollTop) > 0.5) element.scrollTop = want;
      }
    }
    laidOut.current = offsets;
    capture();
    const rel = element.scrollTop - contentTop.current;
    setView((prev) => (Math.abs(prev.top - rel) < QUANTUM && prev.height === element.clientHeight ? prev : { top: rel, height: element.clientHeight }));
  }, [scroller, measureFresh, segments, entries, indexByKey, pinned, following, capture, offsets, stickToEnd, endGoal]);

  const scrollToIndex = useCallback(
    (index: number, align: Align): Promise<HTMLElement | null> => {
      const { entries: rows, offsets: now } = latest.current;
      const row = rows[index];
      const element = scroller.current;
      if (!row || !element) return Promise.resolve(null);
      onUnpin();
      return new Promise((resolve) => {
        goal.current?.resolve(null);
        clearTimeout(goalTimer.current);
        const wanted: Goal = { key: row.key, align, height: null, resolve };
        goal.current = wanted;
        setTarget(row.key);
        const lead = align === "center" ? element.clientHeight / 2 : 0;
        element.scrollTop = Math.max(0, now[index]! + contentTop.current - lead);
        goalTimer.current = setTimeout(() => {
          if (goal.current !== wanted) return;
          wanted.resolve(mounted.current.get(row.key) ?? null);
          endGoal();
        }, GOAL_MS);
      });
    },
    [onUnpin, scroller, endGoal],
  );

  const offsetOfRow = useCallback((index: number): number | null => {
    const { offsets: now } = latest.current;
    return index >= 0 && index < now.length - 1 ? now[index]! + contentTop.current : null;
  }, []);

  return { segments, rowRef, onScroll, stickToEnd, scrollToIndex, offsetOfRow, anchor: () => anchor.current, rev };
}
