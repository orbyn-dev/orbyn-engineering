import { describe, expect, it } from "vitest";
import type { AutopilotHeld } from "../../../generated/AutopilotHeld";
import type { AutopilotLogRow } from "../../../generated/AutopilotLogRow";
import type { AutopilotReviewItem } from "../../../generated/AutopilotReviewItem";
import type { BackgroundCrew } from "../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../generated/BackgroundRun";
import { heldRow } from "../autopilot/approvals/held";
import type { AutopilotAreaRow } from "../../../generated/AutopilotAreaRow";
import { autonomyItems, backgroundItems, heldItems, mainItems, only, rank, stopItems, viewOf, watchItems } from "./assemble";
import type { NeedsItem } from "./model";

const NOW = Date.parse("2026-11-12T15:04:00");

const hold = (over: Partial<AutopilotHeld> = {}): AutopilotHeld => ({
  id: "h1", task_id: "task-1", crew_id: "crew-1", kind: "command", command: "rm -rf build/\nsecond", reason: "Not in the brief", state: "held", readable: true, age_secs: 120, expires_in_secs: 480, ...over,
});
const rowsOf = (...items: AutopilotHeld[]) => items.flatMap((one) => heldRow(one, 600, NOW, null, false) ?? []);

const review = (over: Partial<AutopilotReviewItem> = {}): AutopilotReviewItem => ({
  task_id: "t-main", risk: "high", reasons: [], merge_commit: "9be4410c0ffee", before_commit: "f1a0c63", candidate: "crew/x", files: 1, lines: 2, merged_unix: 1_000, due_unix: null, overdue: false, proof: {}, not_verified: [], cost: {}, watch: "broken", needs_person: "cause unclear", ...over,
});

const wire = (over: Partial<AutopilotLogRow>): AutopilotLogRow => ({ id: 1, task_id: "task-client", crew_id: "c", kind: "tripwire", at: "2026-11-07T14:27:00", paths: [], needs_second_engine: false, body: { wire: "denials", in_a_row: 3, in_a_run: 3 }, ...over });

const crew = { id: "bg", name: "Fix failing CI", reviews_waiting: 1 } as BackgroundCrew;
const bgRun = (over: Partial<BackgroundRun> = {}): BackgroundRun => ({
  id: "r1", crew_id: "bg", state: "needs_review", turns_spent: 1, minutes_spent: 1, budget: { turns: 1, minutes: 1 }, review_waiting: true, review_reason: "Budget reached", started_at: "2026-11-07T13:00:00Z", updated_at: "2026-11-07T14:00:00Z", ...over,
});

const limits = { inARow: 3, inARun: 20 };

describe("each source becomes one inbox row", () => {
  it("a held command that still waits is an approval with Approve and Deny; a decided or expired one is not an item", () => {
    const items = heldItems(rowsOf(hold(), hold({ id: "h2", state: "denied" }), hold({ id: "h3", state: "expired" }), hold({ id: "h4", state: "dismissed" })), NOW);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ id: "held:h1", kind: "approval", title: "rm -rf build/", from: "Task task-1", ref: "h1", risk: null, undo: null, cost: null });
    expect(items[0]?.actions.map((one) => one.label)).toEqual(["Approve", "Deny"]);
    expect(items[0]?.since).toBe(NOW - 120_000);
  });

  it("a trip wire is a stopped task in the words the Approvals tab already uses; other ledger rows are not", () => {
    const items = stopItems([wire({ id: 7 }), wire({ id: 8, body: { wire: "injection" } }), wire({ id: 9, kind: "denied" })], limits);
    expect(items.map((one) => [one.id, one.title, one.from])).toEqual([
      ["stop:7:task-client:2026-11-07T14:27:00", "Stopped · too many denials", "Task task-client"],
      ["stop:8:task-client:2026-11-07T14:27:00", "Stopped · possible injected instructions", "Task task-client"],
    ]);
    expect(items[0]?.actions).toEqual([{ key: "open", label: "Open", primary: true }, { key: "dismiss", label: "Dismiss" }]);
    expect(items[0]?.cost).toBe("3 denials in a row stopped the task. Nothing else is blocked; the other tasks keep going.");
    expect(items[1]?.cost).toBe("A page the worker read contained text that looks like instructions. The task stopped before it ran anything the text asked for.");
    expect(stopItems([wire({ body: { wire: "denials", in_a_row: 1, in_a_run: 20 } })], limits)[0]?.cost).toContain("20 denials in one run");
  });

  it("only a break the crew was not certain about reaches Needs you; a plain after-merge review stays in the Review tab", () => {
    const items = mainItems([review(), review({ task_id: "t-plain", needs_person: null }), review({ task_id: "t-empty", needs_person: "" })]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ id: "main:t-main", kind: "main", risk: "high", from: "Watch on main · 9be4410", cost: "Nothing has been reverted. Main stays as it is until you choose.", since: 1_000_000, overdue: false });
  });

  it("a background run waiting on you is Paused: budget reached, or Needs approval with its reason; a run not waiting is not an item", () => {
    const items = backgroundItems([{ crew, runs: [bgRun({ stop_reason: "budget" }), bgRun({ id: "r2", review_reason: null }), bgRun({ id: "r4", review_reason: "Checks failed twice" }), bgRun({ id: "r3", review_waiting: false })] }]);
    expect(items.map((one) => [one.id, one.kind, one.title, one.cost, one.from, one.ref])).toEqual([
      ["background:r1", "background", "Paused: budget reached", "The run stopped at its budget. The budget cannot be changed; turn the crew off, or create a new crew with a bigger budget.", "Background crew · Fix failing CI", "bg"],
      ["background:r2", "background", "Needs approval", "A command is held for your approval before the run can go on.", "Background crew · Fix failing CI", "bg"],
      ["background:r4", "background", "Needs approval", "Checks failed twice", "Background crew · Fix failing CI", "bg"],
    ]);
    expect(items.every((one) => one.risk === null)).toBe(true);
  });

  it("a watch that turned failing and one whose checks could not run are two rows, each opened at its own watch", () => {
    const items = watchItems({
      items: [
        { id: "bg-watch-redirect", kind: "watch_failing", project_id: "p", crew_id: "bg-watch-redirect", since: "2026-11-07T14:02:00Z", dismissible: false, reason: "The watch’s command is failing." },
        { id: "bg-watch-lock", kind: "watch_could_not_run", project_id: "p", crew_id: "bg-watch-lock", since: "2026-11-07T12:02:00Z", dismissible: false, reason: "3 checks in a row could not run: timed out." },
      ],
      names: { "bg-watch-redirect": "Redirect tests", "bg-watch-lock": "Lockfile check" },
    });
    expect(items.map((one) => [one.id, one.kind, one.title, one.ref])).toEqual([
      ["watch:bg-watch-redirect", "background", "Watch failing: Redirect tests", "bg-watch-redirect"],
      ["watch:bg-watch-lock", "background", "Watch could not run: Lockfile check", "bg-watch-lock"],
    ]);
    expect(items[0]?.from).toMatch(/^Watch · Redirect tests · turned failing /);
    expect(items[1]?.from).toMatch(/^Watch · Lockfile check · 3 checks in a row, since /);
    expect(items[0]?.cost).toBe("The watch’s command is failing. This is raised once for this change and is meant not to repeat while the check keeps failing. It resolves itself when a check passes.");
    expect(items[1]?.cost).toBe("3 checks in a row could not run: timed out. One row for the whole stretch: it is updated at each later check, not repeated, and it resolves itself when a check passes.");
    expect(items.map((one) => one.actions)).toEqual([[{ key: "open", label: "Open", primary: true }], [{ key: "open", label: "Open", primary: true }]]);
    expect(items.every((one) => one.risk === null && one.undo === null)).toBe(true);
  });

  it("a daily-cap stop shows the core's reason verbatim and says to wait for the reset; the lifetime stop keeps its words", () => {
    const why = "Engine alpha reached this crew's daily budget: 3 of its 2 turns today. The budget resets at 2026-11-20T00:00:00+11:00.";
    const unreadable = "The crew's daily budget could not be read, so the run stopped instead of going on.";
    const notUpdated = "The crew's daily budget could not be updated, so the run stopped instead of going on.";
    const items = backgroundItems([{ crew, runs: [
      bgRun({ id: "d1", stop_reason: "budget", review_reason: why }),
      bgRun({ id: "d2", stop_reason: "budget", review_reason: unreadable }),
      bgRun({ id: "d3", stop_reason: "budget", review_reason: notUpdated }),
      bgRun({ id: "l1", stop_reason: "budget", review_reason: "The run reached its budget and stopped." }),
    ] }]);
    expect(items.map((one) => [one.id, one.title, one.cost, one.hint])).toEqual([
      ["background:d1", "Paused: budget reached", why, "This run stays stopped. New runs can start after the daily reset: 2026-11-20T00:00:00+11:00. The budget is fixed at creation."],
      ["background:d2", "Paused: budget reached", unreadable, "No reset time: the daily budget could not be read or updated, so this is not a wait for the reset. Open the run to review it."],
      ["background:d3", "Paused: budget reached", notUpdated, "No reset time: the daily budget could not be read or updated, so this is not a wait for the reset. Open the run to review it."],
      ["background:l1", "Paused: budget reached", "The run stopped at its budget. The budget cannot be changed; turn the crew off, or create a new crew with a bigger budget.", undefined],
    ]);
  });

  it("an area the breaker demoted under a Full ceiling is one autonomy row in the design's words", () => {
    const row: AutopilotAreaRow = {
      area: "web/auth", area_key: "web/auth", ceiling: "full", now: "on", state: "demoted",
      line: "Lowered after 2 of 9 merges were reverted in 14 days.", since: 1_000,
      record: { merges: 9, reverts: 2, window_days: 14 },
    };
    const items = autonomyItems([row], "p-app");
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      id: "autonomy:p-app:web/auth:1000", kind: "autonomy", ref: "web/auth", risk: null, undo: null, since: 1_000_000,
      title: "web/auth moved from Full to On: 2 reverts in 14 days. Review or override.",
      from: "Autopilot by area · circuit breaker · 9 merges in the window",
      cost: "Nothing is blocked. A High risk diff in web/auth now waits for you at the merge gate; Low and Medium still merge by themselves. Dismiss is remembered on this browser.",
    });
    expect(items[0]!.actions).toEqual([
      { key: "review", label: "Review", primary: true },
      { key: "override", label: "Override…" },
      { key: "dismiss", label: "Dismiss" },
    ]);
  });

  it("only a demotion under a Full ceiling is a row: recovering, criteria-met, a lower ceiling, a person's choice and untripped areas are not", () => {
    const row = (over: Partial<AutopilotAreaRow>): AutopilotAreaRow => ({
      area: "web/auth", area_key: "web/auth", ceiling: "full", now: "on", state: "demoted", since: 1_000,
      line: "Lowered after 2 of 9 merges were reverted in 14 days.",
      record: { merges: 9, reverts: 2, window_days: 14 }, ...over,
    });
    expect(autonomyItems([
      row({ state: "recovering", recovery: { clean_days: 7, days_needed: 14, merges: 3, merges_needed: 5 } }),
      row({ state: "criteria_met", recovery: { clean_days: 14, days_needed: 14, merges: 6, merges_needed: 5 } }),
      row({ state: "ceiling_lower", ceiling: "off", now: "off" }),
      row({ state: "overridden", now: "off", override: { choice: "off", at: 5 } }),
      row({ state: "follows", now: "full", since: undefined }),
      row({ state: "not_enough_data", now: "full", since: undefined }),
    ], "p-app")).toEqual([]);
  });

  it("a demotion that left the area Off is not a row: the line's High-waits, Low-and-Medium-merge copy is true only at On", () => {
    const off: AutopilotAreaRow = {
      area: "web/auth", area_key: "web/auth", ceiling: "full", now: "off", state: "demoted", since: 1_000,
      line: "Lowered after 5 of 9 merges were reverted in 14 days.", record: { merges: 9, reverts: 5, window_days: 14 },
    };
    expect(autonomyItems([off], "p-app")).toEqual([]);
    expect(autonomyItems([{ ...off, now: "on" }], "p-app")).toHaveLength(1);
  });

  it("the id names the repository, so the same area and trip in two repositories are two rows", () => {
    const row: AutopilotAreaRow = {
      area: "web/auth", area_key: "web/auth", ceiling: "full", now: "on", state: "demoted", since: 1_000,
      line: "x", record: { merges: 9, reverts: 2, window_days: 14 },
    };
    expect(autonomyItems([row], "p-a")[0]!.id).not.toBe(autonomyItems([row], "p-b")[0]!.id);
  });

  it("nothing in, nothing out", () => {
    expect([...heldItems([], NOW), ...stopItems([], limits), ...mainItems([]), ...backgroundItems([])]).toEqual([]);
    expect(rank([])).toEqual([]);
    expect(viewOf([])).toEqual({ items: [], filters: [] });
  });
});

const make = (id: string, over: Partial<NeedsItem>): NeedsItem => ({ id, kind: "approval", title: id, risk: null, undo: null, from: "", cost: null, since: null, actions: [], ref: id, ...over });

describe("ranking", () => {
  it("puts a stated risk first (High, Medium, Low), then the items without one, each oldest first, an unknown time last", () => {
    const ranked = rank([
      make("unknown-new", { since: 300 }),
      make("low", { risk: "low", since: 1 }),
      make("unknown-old", { since: 100 }),
      make("high", { risk: "high", since: 500 }),
      make("medium", { risk: "medium", since: 5 }),
      make("no-time", {}),
    ]);
    expect(ranked.map((one) => one.id)).toEqual(["high", "medium", "low", "unknown-old", "unknown-new", "no-time"]);
  });

  it("breaks a full tie by id so the order never moves between reads", () => {
    expect(rank([make("b", { since: 1 }), make("a", { since: 1 })]).map((one) => one.id)).toEqual(["a", "b"]);
  });

  it("adds nothing to an item: no row is marked by its place in the list", () => {
    expect(rank([make("a", { since: 1 }), make("b", { since: 2 })]).map((one) => Object.keys(one).sort())).toEqual([Object.keys(make("a", {})).sort(), Object.keys(make("b", {})).sort()]);
  });

  it("does not change the items it was given", () => {
    const list = [make("b", {}), make("a", {})];
    rank(list);
    expect(list.map((one) => one.id)).toEqual(["b", "a"]);
  });
});

describe("ranking", () => {
  it("ranks an autonomy row below every item that waits on the person, whatever its age (needsyou-demoted)", () => {
    const ranked = rank([
      make("auto", { kind: "autonomy", since: 1 }),
      make("hold", { kind: "approval", since: 500 }),
      make("stop", { kind: "stopped", since: 400 }),
      make("main", { kind: "main", risk: "high", since: 300 }),
    ]);
    expect(ranked.map((one) => one.id)).toEqual(["main", "stop", "hold", "auto"]);
  });
});

describe("filters", () => {
  it("lists one chip for each kind that has an item, in the designs' order, with its count", () => {
    const items = [make("1", { kind: "main" }), make("2", { kind: "approval" }), make("3", { kind: "approval" }), make("4", { kind: "stopped" }), make("5", { kind: "background" })];
    expect(viewOf(items).filters).toEqual([
      { kind: "approval", label: "Approvals", count: 2 },
      { kind: "main", label: "Main", count: 1 },
      { kind: "stopped", label: "Stops", count: 1 },
      { kind: "background", label: "Background", count: 1 },
    ]);
    expect(only(items, "approval")).toHaveLength(2);
    expect(only(items, "all")).toHaveLength(5);
  });
});
