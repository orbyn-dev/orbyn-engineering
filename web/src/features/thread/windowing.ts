import type { TimelineEntry } from "../../lib/timeline/timeline";

/** The flex gap between transcript rows, which every row boundary carries. */
export const ROW_GAP = 16;
/** How far past the view, in pixels, rows stay mounted so fast scrolling never shows a hole. */
export const OVERSCAN = 900;
/** Largest run of rows between a selection's two ends that is kept mounted so copy stays whole. */
export const MAX_HELD_BETWEEN = 200;

const LINE_CHARS = 90;

/** A first guess for a row that hasn't been measured yet; replaced by its real height on mount. */
export function estimateRow(entry: TimelineEntry): number {
  switch (entry.kind) {
    case "message": {
      const per = entry.role === "user" ? 60 : LINE_CHARS;
      const lines = entry.text.split("\n").reduce((sum, line) => sum + Math.max(1, Math.ceil(line.length / per)), 0);
      return entry.role === "user" ? 34 + 22 * Math.min(lines, 12) : 12 + 27 * Math.min(lines, 400);
    }
    case "work":
      return entry.items.length > 1 ? 30 : 56;
    case "fold":
      return 34;
    case "working":
      return 60;
    case "approval":
      return 130;
    case "plan":
      return 52 + 24 * entry.steps.length;
    case "diff":
      return 56 + 26 * entry.files.length;
    case "notice":
      return 48;
    default:
      return 60;
  }
}

/** Remembers how tall each row measured, by row key, and at what width the text was laid out. */
export class RowSizes {
  private readonly heights: Map<string, number>;
  private readonly holder: { width: number };
  constructor(heights?: Map<string, number>, holder?: { width: number }) {
    this.heights = heights ?? new Map();
    this.holder = holder ?? { width: 0 };
  }
  get width(): number {
    return this.holder.width;
  }
  set width(value: number) {
    this.holder.width = value;
  }
  get(key: string): number | undefined {
    return this.heights.get(key);
  }
  /** Stores a measurement; true when it moved the layout. */
  set(key: string, height: number): boolean {
    const known = this.heights.get(key);
    if (known !== undefined && Math.abs(known - height) <= 0.5) return false;
    this.heights.set(key, height);
    return true;
  }
  /** Forgets every height except these rows', which are in the page and report their own. */
  retain(keys: Iterable<string>): void {
    const keep = new Map<string, number>();
    for (const key of keys) {
      const height = this.heights.get(key);
      if (height !== undefined) keep.set(key, height);
    }
    this.heights.clear();
    for (const [key, height] of keep) this.heights.set(key, height);
  }
}

interface CachedSizes {
  heights: Map<string, number>;
  holder: { width: number };
}
const caches = new Map<string, CachedSizes>();
const CACHED_THREADS = 8;

/** Row heights per thread, so coming back to a thread places rows where they were. */
export function sizesFor(threadId: string): RowSizes {
  let known = caches.get(threadId);
  if (known) caches.delete(threadId);
  else known = { heights: new Map(), holder: { width: 0 } };
  caches.set(threadId, known);
  for (const old of [...caches.keys()].slice(0, Math.max(0, caches.size - CACHED_THREADS))) caches.delete(old);
  return new RowSizes(known.heights, known.holder);
}

/** `offsets[i]` is the top of row i; the last slot is the end of the final row plus its gap. */
export function buildOffsets(entries: readonly TimelineEntry[], sizes: RowSizes): Float64Array {
  const offsets = new Float64Array(entries.length + 1);
  for (let at = 0; at < entries.length; at++) {
    const entry = entries[at]!;
    offsets[at + 1] = offsets[at]! + (sizes.get(entry.key) ?? estimateRow(entry)) + ROW_GAP;
  }
  return offsets;
}

export const totalHeight = (offsets: Float64Array): number => Math.max(0, offsets[offsets.length - 1]! - ROW_GAP);

/** The row that holds a position, clamped to the rows that exist. */
export function indexAt(offsets: Float64Array, y: number): number {
  const count = offsets.length - 1;
  if (count <= 0) return 0;
  let low = 0;
  let high = count - 1;
  while (low < high) {
    const mid = (low + high + 1) >> 1;
    if (offsets[mid]! <= y) low = mid;
    else high = mid - 1;
  }
  return low;
}

export interface RowRange {
  start: number;
  end: number;
}

/** The rows that touch the view plus the overscan on both sides; `end` is exclusive. */
export function visibleRange(offsets: Float64Array, top: number, height: number, overscan = OVERSCAN): RowRange {
  const count = offsets.length - 1;
  if (count <= 0) return { start: 0, end: 0 };
  const start = indexAt(offsets, top - overscan);
  const end = Math.min(count, indexAt(offsets, top + height + overscan) + 1);
  return { start, end: Math.max(end, start + 1) };
}

export type Segment = { kind: "row"; index: number } | { kind: "gap"; from: number; to: number; height: number };

/**
 * What to mount: the range plus any held rows (focus, selection, a scroll target), in order.
 * Rows left out become one spacer each, sized to the space they would fill, so the scroll
 * height is the same as if every row were mounted.
 */
export function planRows(offsets: Float64Array, range: RowRange, held: Iterable<number>): Segment[] {
  const count = offsets.length - 1;
  const wanted = new Set<number>();
  for (let at = range.start; at < range.end; at++) wanted.add(at);
  for (const at of held) if (at >= 0 && at < count) wanted.add(at);
  const order = [...wanted].sort((a, b) => a - b);
  const plan: Segment[] = [];
  let next = 0;
  const gap = (to: number) => {
    if (to > next) plan.push({ kind: "gap", from: next, to, height: Math.max(0, offsets[to]! - offsets[next]! - ROW_GAP) });
  };
  for (const at of order) {
    gap(at);
    plan.push({ kind: "row", index: at });
    next = at + 1;
  }
  gap(count);
  return plan;
}
