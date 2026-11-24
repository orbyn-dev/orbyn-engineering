import { describe, expect, it } from "vitest";
import type { AutopilotTrust } from "../../../generated/AutopilotTrust";
import type { DashboardHistory } from "./contract";
import { barHeights, compact, costText, costTotals, duration, fill, fromHistory, fromTrust, medianOf, percent, rate, text, waitingKindLabel, whole, workingWhyLabel } from "./model";

const trust = (over: Partial<AutopilotTrust> = {}): AutopilotTrust => ({
  window_days: 60,
  min_sample: 10,
  high_revert_permille: 150,
  total: { merges: 38, reverts: 2, revert_permille: 52, counts: true },
  areas: [{ name: "web/auth", rate: { merges: 15, reverts: 2, revert_permille: 133, counts: true } }],
  engines: [{ name: "claude", rate: { merges: 21, reverts: 1, revert_permille: 47, counts: true } }],
  autonomy_by_area: true,
  truncated: false,
  ...over,
});

const history = (over: Partial<DashboardHistory> = {}): DashboardHistory => ({
  project: "p",
  days: 60,
  as_of: 1_762_500_000,
  min_rate_sample: 10,
  min_median_sample: 5,
  merges: 38,
  unaided: 27,
  reverted: 2,
  reverted_auto: 1,
  reverted_by_person: 1,
  cycle: { median_s: 2460, p90_s: 7500, fastest_s: 360, slowest_s: 13_200 },
  weeks: [{ start: 1_760_000_000, merges: 3, unaided: 2, reverted: 0 }],
  engines: [{ name: "claude", model: "Sonnet 5.5", merges: 21, unaided: 16, reverted: 1, cost: { unit: "tokens", amount: 6_100_000 } }],
  areas: [{ name: "web/auth", merges: 15, unaided: 8, reverted: 2 }],
  reverts: [{ title: "Rotate the signing key", at: 1_759_000_000, by: "person", certain: null }],
  recent: [],
  truncated: false,
  ...over,
});

describe("numbers from outside", () => {
  it("accepts only whole non-negative counts", () => {
    expect(whole(3)).toBe(3);
    for (const bad of [-1, 1.5, NaN, Infinity, "3", null, undefined, {}]) expect(whole(bad)).toBeNull();
  });
  it("caps and cleans text", () => {
    expect(text("a\u0000b\nc")).toBe("a b c");
    expect(text("x".repeat(200))).toHaveLength(60);
    expect(text("x".repeat(200)).endsWith("…")).toBe(true);
    expect(text(42)).toBe("");
  });
  it("shows a rate only from the minimum sample", () => {
    expect(rate(27, 38, 10)).toEqual({ pct: "71%", of: "27 of 38", few: null });
    expect(rate(3, 5, 10)).toEqual({ pct: null, of: "3 of 5", few: "too few for a rate" });
    expect(rate(0, 0, 10).pct).toBeNull();
    expect(percent(1, 0)).toBeNull();
  });
});

describe("formats", () => {
  it("writes durations, amounts and costs in their own unit", () => {
    expect(duration(41 * 60)).toBe("41 min");
    expect(duration(125 * 60)).toBe("2 h 05 min");
    expect(duration(3 * 3600)).toBe("3 h");
    expect(duration(20)).toBe("under 1 min");
    expect(compact(6_100_000)).toBe("6.1M");
    expect(compact(540_000)).toBe("540k");
    expect(compact(812)).toBe("812");
    expect(costText({ unit: "tokens", amount: 6_100_000 })).toBe("6.1M tokens");
    expect(costText({ unit: "toString", amount: 5 })).toBe("5 toString");
    expect(costText({ unit: "", amount: 5 })).toBe("5 units");
  });
  it("sizes bars by the busiest week and never shows a merge as nothing", () => {
    const weeks = [3, 7, 0, 1].map((merges, start) => ({ start, merges, unaided: 0, reverted: 0 }));
    expect(barHeights(weeks)).toEqual([43, 100, 0, 14]);
    expect(fill(2, 4)).toBe(50);
    expect(fill(5, 0)).toBe(0);
  });
});

describe("from the trust record alone", () => {
  it("has the merges, the reverts and the rows, and nothing the record does not hold", () => {
    const model = fromTrust(trust());
    expect(model).toMatchObject({ source: "trust", days: 60, merges: 38, reverted: 2, minRate: 10, unaided: null, cycle: null, weeks: null, reverts: null, recent: null });
    expect(model.engines).toEqual([{ name: "claude", model: null, merges: 21, reverted: 1, unaided: null, cost: null }]);
    expect(model.areas[0]).toMatchObject({ name: "web/auth", merges: 15, reverted: 2, unaided: null });
  });
  it("survives values that are not what the contract says", () => {
    const hostile = { window_days: "x", min_sample: -3, total: null, areas: "no", engines: [null, { name: 7, rate: null }, { name: "<img src=x onerror=alert(1)>", rate: { merges: -5, reverts: 1.5 } }] } as unknown as AutopilotTrust;
    const model = fromTrust(hostile);
    expect(model).toMatchObject({ days: 0, merges: 0, reverted: 0, minRate: 10, areas: [] });
    expect(model.engines.map((row) => [row.name, row.merges, row.reverted])).toEqual([["unknown", 0, 0], ["unknown", 0, 0], ["<img src=x onerror=alert(1)>", 0, 0]]);
  });
  it("caps the rows", () => {
    const many = Array.from({ length: 200 }, (_, i) => ({ name: `a${i}`, rate: { merges: 1, reverts: 0, revert_permille: 0, counts: false } }));
    expect(fromTrust(trust({ areas: many })).areas).toHaveLength(50);
  });
});

describe("from the history read", () => {
  it("carries autonomy, cycle time, cost and the weeks", () => {
    const model = fromHistory(history(), 60);
    expect(model).toMatchObject({ source: "history", days: 60, unaided: 27, revertedAuto: 1, revertedByPerson: 1, minMedian: 5 });
    expect(model.cycle).toEqual({ median: 2460, p90: 7500, fastest: 360, slowest: 13_200 });
    expect(model.engines[0]?.cost).toEqual({ unit: "tokens", amount: 6_100_000 });
    expect(model.weeks).toHaveLength(1);
  });
  it("shows a median only from the minimum sample, and not at all when none was measured", () => {
    expect(medianOf(fromHistory(history(), 60))).toBe(2460);
    expect(medianOf(fromHistory(history({ merges: 4 }), 60))).toBeNull();
    expect(medianOf(fromHistory(history({ cycle: { median_s: null, p90_s: null, fastest_s: null, slowest_s: null } }), 60))).toBeNull();
    expect(medianOf(fromHistory(history({ cycle: null }), 60))).toBeNull();
  });
  it("keeps an unknown word as unknown, and a day count it did not ask for as the one asked", () => {
    const model = fromHistory(history({ days: 45, reverts: [{ title: "t", at: 1, by: "robot" as never, certain: null }], recent: [{ title: "m", engine: "e", at: 1, needed: "weird" as never, checks: "odd" as never }] }), 90);
    expect(model.days).toBe(90);
    expect(model.reverts?.[0]?.by).toBe("unknown");
    expect(model.recent?.[0]).toMatchObject({ needed: "other", checks: "none" });
  });
  it("does not read service words as object keys", () => {
    const model = fromHistory(history({ reverts: [{ title: "t", at: 1, by: "__proto__" as never, certain: null }] }), 60);
    expect(model.reverts?.[0]?.by).toBe("unknown");
  });
  it("survives arrays and objects that are not what the contract says", () => {
    const model = fromHistory({ ...history(), weeks: null, engines: "x", areas: [null], reverts: {}, recent: 3, cycle: 5, merges: "38" } as unknown as DashboardHistory, 60);
    expect(model).toMatchObject({ merges: 0, weeks: [], engines: [], reverts: [], recent: [], cycle: null });
    expect(model.areas).toEqual([{ name: "unknown", merges: 0, reverted: 0, unaided: null }]);
  });
  it("drops a cost with no usable amount", () => {
    const model = fromHistory(history({ engines: [{ name: "codex", model: null, merges: 1, unaided: 1, reverted: 0, cost: { unit: "tokens", amount: -4 } }] }), 60);
    expect(model.engines[0]?.cost).toBeNull();
  });
});

describe("the Right now row", () => {
  it("carries the live rows from the history read and none from the trust record", () => {
    const model = fromHistory(history({
      live: {
        needs_you: [{ kind: "merge_waiting", count: 1 }, { kind: "task_needs_person", count: 2 }],
        working: [{ why: "retry", count: 1 }],
        truncated: true,
      },
    }), 60);
    expect(model.live).toEqual({
      needsYou: [{ kind: "merge_waiting", count: 1 }, { kind: "task_needs_person", count: 2 }],
      working: [{ why: "retry", count: 1 }],
      truncated: true,
    });
    expect(fromTrust(trust()).live).toBeNull();
    expect(fromHistory(history({ live: undefined }), 60).live).toBeNull();
    expect(fromHistory(history({ live: null }), 60).live).toBeNull();
  });

  it("keeps an unknown kind in the order given, drops a zero or unusable count, and keeps an unknown reason", () => {
    const model = fromHistory(history({
      live: {
        needs_you: [
          { kind: "future_kind" as never, count: 2 },
          { kind: "merge_waiting", count: 0 },
          { kind: "task_needs_person", count: 1 },
        ],
        working: [
          { why: "future_why" as never, count: 2 },
          { why: "running", count: -1 },
          { why: "verifying", count: 1 },
        ],
        truncated: false,
      },
    }), 60);
    expect(model.live?.needsYou).toEqual([{ kind: "future_kind", count: 2 }, { kind: "task_needs_person", count: 1 }]);
    expect(model.live?.working).toEqual([{ why: "future_why", count: 2 }, { why: "verifying", count: 1 }]);
  });

  it("gives every wait and reason its own words, and an unknown one its raw word", () => {
    expect(waitingKindLabel("task_needs_person")).toBe("Asked a question");
    expect(waitingKindLabel("merge_waiting")).toBe("A merge that waits");
    expect(waitingKindLabel("manual_check")).toBe("A check only you can make");
    expect(waitingKindLabel("could_not_start")).toBe("A task that could not start");
    expect(waitingKindLabel("could_not_stop")).toBe("A task that could not be stopped");
    expect(waitingKindLabel("tripwire_stop")).toBe("A task stopped by a trip wire");
    expect(waitingKindLabel("worker_stopped_twice")).toBe("A worker that stopped twice");
    expect(waitingKindLabel("watch_failing")).toBe("A watch that is failing");
    expect(waitingKindLabel("watch_could_not_run")).toBe("A watch that could not run");
    expect(waitingKindLabel("background_review")).toBe("A background run to review");
    expect(waitingKindLabel("future_kind")).toBe("future kind");
    expect(workingWhyLabel("running")).toBe("Running");
    expect(workingWhyLabel("verifying")).toBe("Checks running");
    expect(workingWhyLabel("retry")).toBe("Retrying after a failed check");
    expect(workingWhyLabel("fix")).toBe("Fixing a review finding");
    expect(workingWhyLabel("future_why")).toBe("future_why");
  });
});

describe("cost across repositories", () => {
  it("adds one engine in one unit at a time and never joins engines or units", () => {
    const a = fromHistory(history({ engines: [
      { name: "claude", model: null, merges: 1, unaided: 1, reverted: 0, cost: { unit: "tokens", amount: 6_100_000 } },
      { name: "codex", model: null, merges: 1, unaided: 1, reverted: 0, cost: { unit: "tokens", amount: 3_400_000 } },
    ] }), 60);
    const b = fromHistory(history({ engines: [
      { name: "claude", model: null, merges: 1, unaided: 1, reverted: 0, cost: { unit: "tokens", amount: 2_300_000 } },
      { name: "claude", model: null, merges: 1, unaided: 1, reverted: 0, cost: { unit: "credits", amount: 12 } },
    ] }), 60);
    expect(costTotals([a, b])).toEqual([
      { engine: "claude", unit: "tokens", amount: 8_400_000 },
      { engine: "codex", unit: "tokens", amount: 3_400_000 },
      { engine: "claude", unit: "credits", amount: 12 },
    ]);
  });
});
