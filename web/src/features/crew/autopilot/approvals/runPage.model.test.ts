import { describe, expect, it } from "vitest";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../../generated/BackgroundRun";
import type { BackgroundRunDetail } from "../../../../generated/BackgroundRunDetail";
import { runDetailView, runRow } from "./backgroundModel";

/**
 * The run page's State and Spent rows are the Recent runs table's own words (the frame says so), so the two readers are checked
 * against each other on one run per state the page can meet; neither list of words is restated here.
 */
const budget = { turns: 40, minutes: 60 };
const crew = { id: "bg-1", project_id: "p-app", name: "Main build", template: "fix_failing_ci", trigger: { kind: "watch", command: "npm run build", every_hours: 1, on_failing: "template" }, budget, enabled: true, state: "idle", reviews_waiting: 0 } as unknown as BackgroundCrew;
const NOW = new Date("2026-11-07T16:00:00Z");
const base = { id: "r-1", crew_id: "bg-1", turns_spent: 12, minutes_spent: 6, budget, review_waiting: false, started_at: "2026-11-07T14:02:00Z", updated_at: "2026-11-07T14:48:00Z", ended_at: "2026-11-07T14:48:00Z" };
const runOf = (over: Partial<BackgroundRun>): BackgroundRun => ({ ...base, state: "needs_review", ...over }) as unknown as BackgroundRun;
const detailOf = (run: BackgroundRun): BackgroundRunDetail => ({ run, crew_id: "bg-1", name: "Main build", template: "fix_failing_ci" }) as unknown as BackgroundRunDetail;
const fact = (run: BackgroundRun, label: string) => runDetailView(detailOf(run)).facts.find((one) => one.label === label)?.text;
const tableState = (run: BackgroundRun) => {
  const row = runRow(run, crew, NOW);
  return row.reason ? `${row.state} · ${row.reason}` : row.state;
};

describe("the run page against the Recent runs table", () => {
  const cases: [string, BackgroundRun][] = [
    ["a run that is running", runOf({ state: "running", ended_at: null, updated_at: base.started_at })],
    ["a run that stopped at its budget", runOf({ state: "stopped", stop_reason: "budget", turns_spent: 40, minutes_spent: 31 })],
    ["a run that is waiting for review", runOf({ state: "needs_review", review_waiting: true })],
    ["a start the disk guard refused", runOf({ state: "stopped", stop_reason: null, reason_code: "disk_low", turns_spent: 0, minutes_spent: 0 })],
  ];
  it.each(cases)("says the same State for %s", (_name, run) => {
    expect(fact(run, "State")).toBe(tableState(run));
    expect(runDetailView(detailOf(run)).chip.text).toBe(tableState(run));
  });

  it("says Not started · Disk for a refused start, Needs review for a run waiting, Running for a live one", () => {
    expect(fact(cases[3]![1], "State")).toBe("Not started · Disk");
    expect(fact(cases[2]![1], "State")).toBe("Needs review");
    expect(fact(cases[0]![1], "State")).toBe("Running");
  });

  it("draws a dash for the end of a run that is still going, and a time once it ended", () => {
    expect(fact(cases[0]![1], "Ended")).toBe("—");
    expect(fact(cases[1]![1], "Ended")).not.toBe("—");
  });

  it("says Spent as the table's own result for a stopped run", () => {
    const stopped = cases[1]![1];
    expect(fact(stopped, "Spent")).toBe(runRow(stopped, crew, NOW).result);
  });

  it("says whether a review waits", () => {
    expect(fact(cases[2]![1], "Review waiting")).toBe("Yes");
    expect(fact(cases[1]![1], "Review waiting")).toBe("No");
  });
});
