import { describe, expect, it } from "vitest";
import type { MessageItem } from "./projection";
import type { TimelineEntry } from "./timeline";
import { buildOffsets, estimateRow, indexAt, planRows, RowSizes, ROW_GAP, sizesFor, totalHeight, visibleRange } from "./windowing";

const row = (n: number, text = `row ${n}`): MessageItem => ({
  kind: "message", key: `m${n}`, seq: n, ts: "", runId: "r", role: "assistant", text, done: true,
});
const rows = (count: number): TimelineEntry[] => Array.from({ length: count }, (_, n) => row(n));

describe("row windowing model", () => {
  it("measured heights replace estimates, and a change under half a pixel is ignored", () => {
    const sizes = new RowSizes();
    expect(sizes.set("m0", 120)).toBe(true);
    expect(sizes.set("m0", 120.3)).toBe(false);
    expect(sizes.set("m0", 200)).toBe(true);
    const offsets = buildOffsets(rows(2), sizes);
    expect(offsets[1]).toBe(200 + ROW_GAP);
    expect(offsets[2]).toBe(200 + ROW_GAP + estimateRow(row(1)) + ROW_GAP);
  });

  it("estimates a long message taller than a short one", () => {
    expect(estimateRow(row(0, "x".repeat(900)))).toBeGreaterThan(estimateRow(row(0, "short")) * 3);
  });

  it("finds the row under a position, clamped to the ends", () => {
    const offsets = buildOffsets(rows(10), new RowSizes(new Map(Array.from({ length: 10 }, (_, n) => [`m${n}`, 84]))));
    expect(indexAt(offsets, -50)).toBe(0);
    expect(indexAt(offsets, 99)).toBe(0);
    expect(indexAt(offsets, 100)).toBe(1);
    expect(indexAt(offsets, 1e9)).toBe(9);
  });

  it("selects the view plus the overscan and nothing else", () => {
    const sizes = new RowSizes(new Map(Array.from({ length: 1000 }, (_, n) => [`m${n}`, 84])));
    const offsets = buildOffsets(rows(1000), sizes);
    const { start, end } = visibleRange(offsets, 50_000, 700, 500);
    expect(start).toBe(indexAt(offsets, 49_500));
    expect(end).toBe(indexAt(offsets, 51_200) + 1);
    expect(end - start).toBeLessThan(25);
  });

  it("fills every row left out with a spacer so the total height never changes", () => {
    const sizes = new RowSizes(new Map(Array.from({ length: 100 }, (_, n) => [`m${n}`, 40 + (n % 7) * 10])));
    const offsets = buildOffsets(rows(100), sizes);
    for (const held of [[], [3], [3, 97], [50]]) {
      const plan = planRows(offsets, { start: 40, end: 45 }, held);
      // Flow layout: every segment is followed by one gap, and the last has none.
      const mounted = plan.reduce((sum, segment) => sum + (segment.kind === "gap" ? segment.height : offsets[segment.index + 1]! - offsets[segment.index]! - ROW_GAP), 0);
      expect(mounted + (plan.length - 1) * ROW_GAP).toBeCloseTo(totalHeight(offsets), 6);
      const covered = plan.flatMap((segment) => (segment.kind === "row" ? [segment.index] : []));
      expect(covered).toEqual([...new Set([40, 41, 42, 43, 44, ...held])].sort((a, b) => a - b));
    }
  });

  it("keeps heights per thread for the next visit and forgets old threads", () => {
    sizesFor("a").set("m0", 321);
    expect(sizesFor("a").get("m0")).toBe(321);
    for (let n = 0; n < 12; n++) sizesFor(`other-${n}`);
    expect(sizesFor("a").get("m0")).toBeUndefined();
  });
});
