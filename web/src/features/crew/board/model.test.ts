import { describe, expect, it } from "vitest";
import { boardPath, columnPath } from "./api";
import { ago, COLUMNS, effortLabel, modelLine, moreOf } from "./model";
import { cardItems } from "./CardMenu";

describe("the board's words", () => {
  it("lists the seven columns in the order work moves, with Needs you the only amber", () => {
    expect(COLUMNS.map((one) => one.label)).toEqual(["Queued", "Routed", "Running", "Verifying", "Needs you", "Passed", "Stopped · Cancelled"]);
    expect(COLUMNS.filter((one) => one.tone === "warn").map((one) => one.id)).toEqual(["needs_you"]);
    expect(COLUMNS.filter((one) => one.rail).map((one) => one.id)).toEqual(["passed", "stopped_cancelled"]);
  });

  it("says when a task last changed, after the word updated", () => {
    const now = Date.parse("2026-11-07T15:00:00Z");
    expect(ago("2026-11-07T15:00:00Z", now)).toBe("now");
    expect(ago("2026-11-07T14:59:00Z", now)).toBe("1m ago");
    expect(ago("2026-11-07T12:00:00Z", now)).toBe("3h ago");
    expect(ago("2026-11-04T15:00:00Z", now)).toBe("3d ago");
    expect(ago("2099-01-01T00:00:00Z", now)).toBe("now");
    expect(ago("not a time", now)).toBe("not a time");
  });

  it("shows an effort of Low or Medium only, never another", () => {
    expect(effortLabel("low")).toBe("Low");
    expect(effortLabel("medium")).toBe("Medium");
    expect(effortLabel("high")).toBeNull();
    expect(effortLabel(null)).toBeNull();
    expect(modelLine({ model: "Sonnet 5.5", effort: "medium" })).toBe("Sonnet 5.5 · Medium");
    expect(modelLine({ model: "GPT-5.5", effort: "high" })).toBe("GPT-5.5");
    expect(modelLine({ model: null, effort: null })).toBe("");
  });

  it("counts what a column still has to read from its real total", () => {
    expect(moreOf(12, 8)).toBe(4);
    expect(moreOf(3, 3)).toBe(0);
    expect(moreOf(2, 5)).toBe(0);
  });

  it("builds the board read, and a column's next page, with the filters", () => {
    expect(boardPath({ crew: "c 1", engine: null, kind: null })).toBe("/api/crew/flow/board?crew=c+1");
    expect(columnPath({ crew: "c", engine: "codex", kind: "test" }, "running", 8)).toBe("/api/crew/flow/board?crew=c&engine=codex&kind=test&column=running&limit=50&offset=8");
  });

  it("draws Cancel and Re-run checks only for a task they could apply to, and every item inert", () => {
    const base = { state: "running", can_cancel: false, can_rerun: false };
    expect(cardItems(base).map((item) => item.label)).toEqual(["Reassign engine", "Message into a lane", "Add reviewer lane", "Propose a check"]);
    expect(cardItems({ ...base, can_cancel: true }).map((item) => item.label)[0]).toBe("Cancel");
    expect(cardItems({ ...base, can_rerun: true }).map((item) => item.label)).toContain("Re-run checks");
    expect(cardItems({ ...base, state: "needs_person" }).map((item) => item.label)).toContain("Answer a worker’s question");
    expect(cardItems({ ...base, state: "running" }).map((item) => item.label)).not.toContain("Answer a worker’s question");
    for (const item of cardItems({ state: "needs_person", can_cancel: true, can_rerun: true })) {
      expect(item.disabled).toBe(true);
      expect(item.onSelect).toBeUndefined();
    }
  });
});
