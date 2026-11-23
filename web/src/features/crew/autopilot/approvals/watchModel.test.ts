import { describe, expect, it } from "vitest";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import { watchBanner, watchChip, watchFacts, watchLastLine, watchRow, watchStartsWhen, watchTail, watchView, everyHours } from "./watchModel";
import { clockOf, dayOfIso, plusHours, whenMoment, whenShort } from "./when";
import type { BackgroundWatchChange, BackgroundWatchStatus } from "./watchTypes";

const NOW = new Date("2026-11-07T18:00:00Z");
const crew = (over: Partial<BackgroundCrew> = {}): BackgroundCrew => ({
  id: "bg-watch",
  project_id: "p-app",
  name: "Redirect tests",
  template: "fix_failing_ci",
  trigger: { kind: "watch", command: "npm test web/nav/redirect", every_hours: 1, on_failing: "needs_you" },
  budget: { turns: 40, minutes: 60 },
  daily: { runs: 2, turns: 80 },
  enabled: true,
  state: "idle",
  reviews_waiting: 0,
  created_at: "2026-11-01T10:00:00Z",
  updated_at: "2026-11-07T14:00:00Z",
  ...over,
});
const status = (over: Partial<BackgroundWatchStatus> = {}): BackgroundWatchStatus => ({ crew_id: "bg-watch", state: "passing", baseline_done: true, error_streak: 0, ...over });
const change = (id: number, kind: BackgroundWatchChange["kind"], action: BackgroundWatchChange["action"], at: string, exit_code: number | null, tail: string | null = null): BackgroundWatchChange => ({ id, kind, action, at, exit_code, tail });
const BASELINE = change(1, "baseline", "none", "2026-11-05T09:02:00Z", 0);
const RAISED_1402 = change(4, "failing", "needs_you", "2026-11-07T14:02:00Z", 1);
const RECOVERED_YESTERDAY = change(3, "passing", "resolved", "2026-11-06T09:02:00Z", 0);
const RAISED_THURSDAY = change(2, "failing", "needs_you", "2026-11-05T16:02:00Z", 1);
const CHANGES = [RAISED_1402, RECOVERED_YESTERDAY, RAISED_THURSDAY, BASELINE];

describe("a watch's words", () => {
  it("says the schedule and what a failing result does", () => {
    expect(everyHours(1)).toBe("Every 1 hour");
    expect(everyHours(24)).toBe("Every 24 hours");
    expect(watchStartsWhen(crew())).toBe("Every 1 hour · raises Needs you");
    expect(watchStartsWhen(crew({ trigger: { kind: "watch", command: "npm run build", every_hours: 6, on_failing: "template" } }))).toContain("Every 6 hours · starts Fix failing CI");
  });

  it("draws the state chip the design draws, Paused before any state when the switch is off, and Not checked yet before the baseline", () => {
    expect(watchChip(crew(), null)).toEqual({ text: "Not checked yet", tone: "dim" });
    expect(watchChip(crew(), status({ baseline_done: false }))).toEqual({ text: "Not checked yet", tone: "dim" });
    expect(watchChip(crew(), status({ state: "passing" }))).toEqual({ text: "Passing", tone: "ok" });
    expect(watchChip(crew(), status({ state: "failing" }))).toEqual({ text: "Failing", tone: "no" });
    expect(watchChip(crew(), status({ state: "could_not_run" }))).toEqual({ text: "Could not run", tone: "am" });
    expect(watchChip(crew({ enabled: false }), status({ state: "failing" }))).toEqual({ text: "Paused", tone: "dim" });
    // A watch created off has never checked: the saved frame says Not checked yet, not Paused.
    expect(watchChip(crew({ enabled: false }), status({ baseline_done: false, state: "not_checked" }))).toEqual({ text: "Not checked yet", tone: "dim" });
  });

  it("draws the list's check line in each state, with the failing since a later check did not change", () => {
    expect(watchLastLine(crew(), null, [], NOW)).toBe("Not checked yet");
    expect(watchLastLine(crew({ enabled: false }), status({ state: "failing", last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }), [], NOW)).toMatch(/^Last checked /);
    /** The check line's time is the clock for today and the day before that: the same rule `whenShort` draws. */
    const check = (iso: string) => whenShort(iso, NOW);
    expect(watchLastLine(crew(), status({ state: "passing", last_checked_at: "2026-11-07T10:02:00Z", last_exit_code: 0 }), [BASELINE], NOW)).toBe(`Checked ${check("2026-11-07T10:02:00Z")} · next ${whenShort(plusHours("2026-11-07T10:02:00Z", 1), NOW)}`);
    expect(watchLastLine(crew(), status({ state: "could_not_run", last_checked_at: "2026-11-07T12:02:00Z", reason: "timed out", error_streak: 3 }), [], NOW)).toBe(`Checked ${check("2026-11-07T12:02:00Z")} · could not run: timed out`);
    expect(watchLastLine(crew(), status({ state: "failing", last_checked_at: "2026-11-07T16:02:00Z", last_exit_code: 2 }), CHANGES, NOW)).toBe(`Checked ${check("2026-11-07T16:02:00Z")} · exit 2 · failing since ${check("2026-11-07T14:02:00Z")}`);
    // The check that turned it failing is the change itself: there is no "since" to add.
    expect(watchLastLine(crew(), status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1 }), CHANGES, NOW)).toBe(`Checked ${check("2026-11-07T14:02:00Z")} · exit 1`);
  });

  it("draws one list row from the two reads", () => {
    const row = watchRow(crew(), status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1 }), CHANGES, NOW);
    expect(row).toMatchObject({ chip: { text: "Failing", tone: "no" }, subLine: "Watch · no model · npm test web/nav/redirect", startsWhen: "Every 1 hour · raises Needs you" });
    expect(row.lastLine).toBe(`Checked ${clockOf("2026-11-07T14:02:00Z")} · exit 1`);
  });
});

describe("a watch's page", () => {
  it("a quiet pass says why nothing is raised and names the baseline", () => {
    const banner = watchBanner(crew(), status({ state: "passing", last_checked_at: "2026-11-07T10:02:00Z", last_exit_code: 0 }), [RECOVERED_YESTERDAY, RAISED_THURSDAY, BASELINE], NOW);
    expect(banner).toMatchObject({ title: "Passing", action: null });
    expect(banner.text).toContain("nothing is raised and no run starts");
    expect(banner.text).toContain(dayOfIso(BASELINE.at));
  });

  it("a pass right after a failing change is Passing again, names the row it resolved, and offers nothing", () => {
    const banner = watchBanner(crew(), status({ state: "passing", last_checked_at: "2026-11-07T17:02:00Z", last_exit_code: 0 }), [change(5, "passing", "resolved", "2026-11-07T17:02:00Z", 0), RAISED_1402, RECOVERED_YESTERDAY, BASELINE], NOW);
    expect(banner.title).toBe("Passing again");
    expect(banner.text).toContain(`was failing from ${whenMoment(RAISED_1402.at, NOW)} to ${clockOf("2026-11-07T17:02:00Z")}`);
    expect(banner.text).toContain("resolved that Needs you row");
    expect(banner.action).toBeNull();
  });

  it("a change to failing raises once and offers Open in Needs you", () => {
    const banner = watchBanner(crew(), status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1 }), CHANGES, NOW);
    expect(banner.title).toBe("Failing: raised in Needs you");
    expect(banner.text).toContain("turned failing on");
    expect(banner.text).toContain("raised once, in Needs you");
    expect(banner.action).toBe("needs_you");
  });

  it("a change that started the template names it and offers Open the run", () => {
    const banner = watchBanner(crew({ template: "fix_failing_ci" }), status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1 }), [change(4, "failing", "started_template", "2026-11-07T14:02:00Z", 1)], NOW);
    expect(banner.title).toBe("Failing: started Fix failing CI");
    expect(banner.text).toContain("started one run of the Fix failing CI template");
    expect(banner.action).toBe("run");
  });

  it("a later failing check is Still failing and raises nothing new", () => {
    const banner = watchBanner(crew(), status({ state: "failing", last_checked_at: "2026-11-07T16:02:00Z", last_exit_code: 2 }), CHANGES, NOW);
    expect(banner.title).toBe("Still failing: already raised");
    expect(banner.text).toContain(`It raised once, at ${clockOf(RAISED_1402.at)}`);
    expect(banner.action).toBeNull();
  });

  it("a watch that is off says no check runs and what the next check is compared with", () => {
    const banner = watchBanner(crew({ enabled: false }), status({ state: "failing", last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }), [change(6, "turned_off", "none", "2026-11-07T09:30:00Z", null), change(5, "failing", "needs_you", "2026-11-06T15:04:00Z", 1), BASELINE], NOW);
    expect(banner.title).toBe("Paused: it is off");
    expect(banner.text).toContain("No check runs while it is off.");
    expect(banner.text).toContain("compared with that last result");
  });

  it("a check that could not run is its own state, and three in a row raise one row", () => {
    const banner = watchBanner(crew(), status({ state: "could_not_run", last_checked_at: "2026-11-07T12:02:00Z", reason: "timed out", error_streak: 3 }), [], NOW);
    expect(banner.title).toBe("Could not run: timed out");
    expect(banner.text).toContain("This is not a failing result.");
    expect(banner.text).toContain("one row is in Needs you");
    expect(banner.action).toBeNull();
  });

  it("a watch with no baseline yet says the first check is a baseline", () => {
    const banner = watchBanner(crew(), status({ state: "not_checked", baseline_done: false }), [], NOW);
    expect(banner.title).toBe("Not checked yet");
    expect(banner.text).toContain("never raises, even if it fails");
  });

  it("draws the facts box in the design's order, with the command, the checkout, the schedule and what it keeps", () => {
    const facts = watchFacts(crew(), status({ state: "passing", last_checked_at: "2026-11-07T10:02:00Z", last_exit_code: 0 }), [RECOVERED_YESTERDAY, BASELINE], NOW, "orbyn-engineering");
    expect(facts.map((one) => one.label)).toEqual(["Command", "Runs in", "Checks", "When it turns failing", "Model", "Last checked", "Next check", "Kept"]);
    expect(facts[0]?.text).toBe("npm test web/nav/redirect");
    expect(facts[1]?.text).toBe("orbyn-engineering · this device · the project’s checkout root");
    expect(facts[2]?.text).toBe("Every 1 hour · no network · 5 minutes at most");
    expect(facts[3]?.text).toBe("Raise Needs you");
    expect(facts[4]?.text).toBe("None. A check makes no model call.");
    expect(facts[5]?.text).toContain("exit 0");
    expect(facts.at(-1)?.text).toContain("2 KiB at most, secrets hidden");
  });

  it("names the row an earlier pass resolved, and for a watch that is off says it checks no more", () => {
    const recovered = watchFacts(crew(), status({ state: "passing", last_checked_at: "2026-11-07T17:02:00Z", last_exit_code: 0 }), [change(5, "passing", "resolved", "2026-11-07T17:02:00Z", 0), RAISED_1402, BASELINE], NOW, "orbyn-engineering");
    expect(recovered.find((one) => one.label === "Needs you")?.text).toBe(`The row from ${whenMoment(RAISED_1402.at, NOW)} is resolved`);
    const off = watchFacts(crew({ enabled: false }), status({ state: "failing", last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }), [BASELINE], NOW, "orbyn-engineering");
    expect(off.find((one) => one.label === "Next check")?.text).toBe("None while it is off");
  });

  it("lists only changes of result, says what each did, and names an exit it has no number for as a dash", () => {
    const changes = [watchView(crew(), status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1 }), [RAISED_1402, RECOVERED_YESTERDAY, RAISED_THURSDAY, BASELINE], NOW, "orbyn-engineering").changes];
    expect(changes[0]?.rows.map((one) => one.change)).toEqual(["Passing → Failing", "Passing again", "Passing → Failing", "Baseline: Passing"]);
    expect(changes[0]?.rows.map((one) => one.did)).toEqual([
      "Raised in Needs you.",
      `Resolved the Needs you row from ${whenMoment(RAISED_THURSDAY.at, NOW)}.`,
      "Raised in Needs you.",
      "Recorded. A first check never raises.",
    ]);
    expect(changes[0]?.rows.map((one) => one.exit)).toEqual(["1", "0", "1", "0"]);
    const off = watchView(crew({ enabled: false }), status({ state: "failing", last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }), [change(6, "turned_off", "none", "2026-11-07T09:30:00Z", null), BASELINE], NOW, "orbyn-engineering");
    expect(off.changes.rows[0]).toMatchObject({ change: "Turned off by you", exit: "—", did: "No more checks." });
  });

  it("keeps the latest output of a failing check and draws none for a quiet pass", () => {
    const tail = watchTail(status({ state: "failing", last_checked_at: "2026-11-07T14:02:00Z", last_exit_code: 1, tail: "FAIL web/nav/redirect.test.ts" }));
    expect(tail?.title).toBe(`Latest output · the check at ${clockOf("2026-11-07T14:02:00Z")} · exit 1 · the last 20 lines, 2 KiB at most, secrets hidden`);
    expect(tail?.body).toBe("FAIL web/nav/redirect.test.ts");
    expect(watchTail(status({ state: "passing", last_checked_at: "2026-11-07T10:02:00Z", tail: "ok" }))).toBeNull();
    expect(watchTail(status({ state: "could_not_run", last_checked_at: "2026-11-07T12:02:00Z", reason: "timed out" }))).toBeNull();
  });
});
