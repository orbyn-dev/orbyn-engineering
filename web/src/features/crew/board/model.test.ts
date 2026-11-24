import { describe, expect, it, vi } from "vitest";
import { boardPath, columnPath } from "./api";
import { ago, checksLine, COLUMNS, effortLabel, modelLine, moreOf, waitingLine, workingLine } from "./model";
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

  it("says the checks passed from two counts, and nothing for an absent or impossible pair", () => {
    expect(checksLine({ checks: { passed: 2, of: 3 } })).toBe("2 of 3 checks passed");
    expect(checksLine({ checks: { passed: 1, of: 1 } })).toBe("1 of 1 check passed");
    expect(checksLine({ checks: { passed: 0, of: 2 } })).toBe("0 of 2 checks passed");
    expect(checksLine({ checks: null })).toBeNull();
    expect(checksLine({})).toBeNull();
    for (const [passed, of] of [[0, 0], [3, 2], [-1, 2], [1.5, 2], [Number.NaN, 2]]) {
      expect(checksLine({ checks: { passed: passed!, of: of! } })).toBeNull();
    }
  });

  it("says why a worker owns live work, and nothing for a value it does not know", () => {
    expect(workingLine("running")).toBe("Working");
    expect(workingLine("verifying")).toBe("Working · checks running");
    expect(workingLine("retry")).toBe("Working · retrying after a failed check");
    expect(workingLine("fix")).toBe("Working · fixing a review finding");
    expect(workingLine(null)).toBeNull();
    expect(workingLine(undefined)).toBeNull();
    expect(workingLine("future")).toBeNull();
  });

  it("says the one needs-you sentence by kind, and keeps the reason for a kind it does not know", () => {
    const question = "Which target wins when both next and returnTo are set?";
    expect(waitingLine({ needs_you: "task_needs_person", reason: question })).toEqual({ lead: "Asked you a question", text: question });
    expect(waitingLine({ needs_you: "task_needs_person", reason: null })).toEqual({ lead: "Asked you a question" });
    expect(waitingLine({ needs_you: "merge_waiting", reason: "A merge needs your decision." })).toEqual({ text: "The merge waits for you" });
    expect(waitingLine({ needs_you: "manual_check", reason: "A check only a person can make is waiting." })).toEqual({ text: "A check only a person can make is waiting." });
    expect(waitingLine({ needs_you: "background_review", reason: "The review waits for you." })).toEqual({ text: "The review waits for you." });
    expect(waitingLine({ needs_you: "future_kind", reason: "The reason still shows." })).toEqual({ text: "The reason still shows." });
    expect(waitingLine({ needs_you: null, reason: "The field does not name it." })).toEqual({ text: "The field does not name it." });
    expect(waitingLine({ needs_you: "merge_waiting" })).toEqual({ text: "The merge waits for you" });
    expect(waitingLine({ needs_you: "manual_check", reason: null })).toBeNull();
    expect(waitingLine({ needs_you: "future_kind", reason: null })).toBeNull();
    expect(waitingLine({})).toBeNull();
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

  it("draws Cancel and Re-run checks only for a task they could apply to, enabled by the write state, and the rest inert", () => {
    const base = { state: "running", can_cancel: false, can_rerun: false };
    const on = { can: () => ({ enabled: true, why: null }), ask: vi.fn() };
    const off = { can: () => ({ enabled: false, why: "Waiting for the answer" }), ask: vi.fn() };
    expect(cardItems(base, on).map((item) => item.label)).toEqual(["Reassign engine", "Message into a lane", "Add reviewer lane", "Propose a check"]);
    expect(cardItems({ ...base, can_cancel: true }, on).map((item) => item.label)[0]).toBe("Cancel");
    expect(cardItems({ ...base, can_rerun: true }, on).map((item) => item.label)).toContain("Re-run checks");
    expect(cardItems({ ...base, state: "needs_person" }, on).map((item) => item.label)).toContain("Answer a worker’s question");
    expect(cardItems({ ...base, state: "running" }, on).map((item) => item.label)).not.toContain("Answer a worker’s question");
    // Cancel is a choice only while the write state allows it; choosing it asks, and sends nothing.
    const [cancel] = cardItems({ ...base, can_cancel: true }, on);
    expect(cancel!.disabled).toBeUndefined();
    cancel!.onSelect!();
    expect(on.ask).toHaveBeenCalledWith("cancel");
    expect(cardItems({ ...base, can_cancel: true }, off)[0]).toMatchObject({ label: "Cancel", disabled: true });
    // Every item that is target design stays inert whatever the state.
    const inert = cardItems({ state: "needs_person", can_cancel: true, can_rerun: true }, on).filter((item) => !["Cancel", "Re-run checks"].includes(item.label));
    expect(inert.map((item) => item.label)).toEqual(["Reassign engine", "Message into a lane", "Add reviewer lane", "Answer a worker’s question", "Propose a check"]);
    for (const item of inert) {
      expect(item.disabled).toBe(true);
      expect(item.onSelect).toBeUndefined();
    }
  });
});
