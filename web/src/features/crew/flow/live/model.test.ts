import { describe, expect, it } from "vitest";
import type { FlowCell } from "../../../../generated/FlowCell";
import type { FlowTaskRow } from "../../../../generated/FlowTaskRow";
import { tasksPath } from "./api";
import { cellOf, cellView, countOf, decidedBy, laneWord, roundView, shown, stageWord, STAGES, when } from "./model";

const row = (over: Partial<FlowTaskRow>): FlowTaskRow => ({ id: "t", name: "n", kind: "implement", owner: "Lead", state: "running", round: 1, cells: [], created_at: "", updated_at: "", ...over });

describe("the cells of a task's line", () => {
  it("writes checks as passed over run and lanes as a count, and names the state for a screen reader", () => {
    expect(cellView({ stage: "checks", state: "running", passed: 2, of: 3 }, "Checks")).toEqual({ tone: "run", glyph: "spin", text: "2/3", label: "Checks: Running 2/3" });
    expect(cellView({ stage: "code", state: "done", lanes: 3 }, "Code review")).toMatchObject({ tone: "ok", text: "×3", label: "Code review: Done ×3" });
    expect(cellView({ stage: "security", state: "failed", lanes: 2 }, "Security review")).toMatchObject({ tone: "fail", glyph: "alert", text: "×2" });
  });

  it("draws a skipped stage as a dash, a not-reached one empty and a review waiting after the merge with an eye", () => {
    expect(cellView({ stage: "security", state: "skipped" }, "Security review")).toMatchObject({ tone: "skip", text: "–", label: "Security review: Skipped" });
    expect(cellView({ stage: "main", state: "pending" }, "Main")).toMatchObject({ tone: "pend", text: "", glyph: null, label: "Main: Not reached" });
    expect(cellView({ stage: "you", state: "review_waiting" }, "You")).toMatchObject({ tone: "rev", glyph: "eye" });
  });

  it("never shows a count of zero lanes, and a missing cell reads as not reached", () => {
    expect(cellView({ stage: "code", state: "pending", lanes: 0 }, "Code review").text).toBe("");
    expect(cellOf(row({ cells: [] }), "main")).toEqual<FlowCell>({ stage: "main", state: "pending" });
  });

  it("lists the ten stages in the order of the line", () => {
    expect(STAGES.map((one) => one.stage)).toEqual(["brief", "route", "build", "checks", "code", "security", "finding", "merge", "main", "you"]);
  });
});

describe("the Round column", () => {
  it("is Queued before the task starts, blue while it runs, red when it failed or stopped", () => {
    expect(roundView(row({ state: "briefed" }))).toEqual({ tone: "plain", text: "Queued" });
    expect(roundView(row({ state: "verifying", round: 2 }))).toEqual({ tone: "run", text: "Round 2" });
    expect(roundView(row({ state: "failed", round: 3 }))).toEqual({ tone: "fail", text: "Round 3" });
    expect(roundView(row({ state: "stopped" })).tone).toBe("fail");
    expect(roundView(row({ state: "passed" })).tone).toBe("plain");
  });
});

describe("the words of the zoomed stage list", () => {
  it("uses Crew Designs' own words per stage and state", () => {
    expect(stageWord("brief", { stage: "brief", state: "done" })).toEqual({ tone: "ok", text: "Written" });
    expect(stageWord("checks", { stage: "checks", state: "done", passed: 3, of: 3 })).toEqual({ tone: "ok", text: "3 of 3" });
    expect(stageWord("code", { stage: "code", state: "running", lanes: 2 })).toEqual({ tone: "info", text: "Reviewing" });
    expect(stageWord("merge", { stage: "merge", state: "pending" })).toEqual({ tone: "neutral", text: "Waiting" });
    expect(stageWord("security", { stage: "security", state: "skipped" }).text).toBe("Skipped");
    expect(stageWord("build", { stage: "build", state: "needs_you" })).toEqual({ tone: "warn", text: "Needs you" });
    expect(stageWord("finding", { stage: "finding", state: "failed" })).toEqual({ tone: "err", text: "Failed" });
  });

  it("words a lane by its findings, else by its state, and a gate by who decided it", () => {
    const lane = { lane: "l", group: "code", task: "t", round: 1, state: "passed", findings: 0, confirmed: 0, unconfirmed: 0 };
    expect(laneWord(lane)).toBe("No findings");
    expect(laneWord({ ...lane, state: "running" })).toBe("Reviewing");
    expect(laneWord({ ...lane, unconfirmed: 1 })).toBe("1 unconfirmed");
    expect(laneWord({ ...lane, confirmed: 2, unconfirmed: 1 })).toBe("2 confirmed");
    const gate = { stage: "finding", decision: "blocks", lane: null, reason: null, at: "" };
    expect(decidedBy({ ...gate, decided_by: "code" })).toBe("Code");
    expect(decidedBy({ ...gate, decided_by: "confirmed_finding" })).toBe("A confirmed finding");
    expect(decidedBy({ ...gate, decided_by: "person" })).toBe("You");
  });
});

describe("the list's chips, counts and read", () => {
  it("takes a chip's count from the counts over the whole crew", () => {
    const counts = { all: 31, needs_you: 2, reviewing: 5, failed: 2, passed: 5 };
    expect(["all", "needs_you", "reviewing", "failed", "passed"].map((filter) => countOf(counts, filter as never))).toEqual([31, 2, 5, 2, 5]);
    expect(shown(8, 31)).toBe("8 of 31 shown");
  });

  it("leaves out a filter of all, an empty search and no engine, and encodes the rest", () => {
    expect(tasksPath({ crew: "c 1", filter: "all", q: " ", engine: null, offset: 0 })).toBe("/api/crew/flow/tasks?crew=c+1&limit=50");
    expect(tasksPath({ crew: "c", filter: "needs_you", q: " login ", engine: "codex", offset: 50 })).toBe("/api/crew/flow/tasks?crew=c&limit=50&filter=needs_you&q=login&engine=codex&offset=50");
  });

  it("reads a moment on the clock and shows an unreadable one as sent", () => {
    expect(when("not a time")).toBe("not a time");
    expect(when("2026-11-07T14:02:00Z")).toMatch(/Nov/);
  });
});
