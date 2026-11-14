import { describe, expect, it } from "vitest";
import type { AutopilotHeld } from "../../../generated/AutopilotHeld";
import type { AutopilotLogRow } from "../../../generated/AutopilotLogRow";
import type { AutopilotReviewItem } from "../../../generated/AutopilotReviewItem";
import type { BackgroundCrew } from "../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../generated/BackgroundRun";
import { heldRow } from "../autopilot/approvals/held";
import { backgroundItems, heldItems, mainItems, only, rank, stopItems, viewOf } from "./assemble";
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
      ["stop:7", "Stopped · too many denials", "Task task-client"],
      ["stop:8", "Stopped · possible injected instructions", "Task task-client"],
    ]);
    expect(items[0]?.actions).toEqual([{ key: "open", label: "Open", primary: true }]);
  });

  it("only a break the crew was not certain about reaches Needs you; a plain after-merge review stays in the Review tab", () => {
    const items = mainItems([review(), review({ task_id: "t-plain", needs_person: null }), review({ task_id: "t-empty", needs_person: "" })]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ id: "main:t-main", kind: "main", risk: "high", from: "Watch on main · 9be4410", cost: "Nothing has been reverted. Main stays as it is until you choose.", since: 1_000_000 });
  });

  it("a background run waiting for review is a Review row named for its reason, or the crew; a run not waiting is not", () => {
    const items = backgroundItems([{ crew, runs: [bgRun(), bgRun({ id: "r2", review_reason: null }), bgRun({ id: "r3", review_waiting: false })] }]);
    expect(items.map((one) => [one.id, one.title, one.from, one.ref])).toEqual([
      ["background:r1", "Review: Budget reached", "Background · Fix failing CI", "bg"],
      ["background:r2", "Review: Fix failing CI", "Background · Fix failing CI", "bg"],
    ]);
    expect(items.every((one) => one.kind === "review" && one.risk === null)).toBe(true);
  });

  it("nothing in, nothing out", () => {
    expect([...heldItems([], NOW), ...stopItems([], limits), ...mainItems([]), ...backgroundItems([])]).toEqual([]);
    expect(rank([])).toEqual([]);
    expect(viewOf([])).toEqual({ items: [], filters: [] });
  });
});

const make = (id: string, over: Partial<NeedsItem>): NeedsItem => ({ id, kind: "approval", title: id, risk: null, undo: null, from: "", cost: null, since: null, hi: false, actions: [], ref: id, ...over });

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

  it("marks the top two amber, unless they are Low risk", () => {
    expect(rank([make("a", { since: 1 }), make("b", { since: 2 }), make("c", { since: 3 })]).map((one) => one.hi)).toEqual([true, true, false]);
    expect(rank([make("a", { risk: "low" }), make("b", { risk: "low", id: "b" })]).map((one) => one.hi)).toEqual([false, false]);
  });

  it("does not change the items it was given", () => {
    const list = [make("b", {}), make("a", {})];
    rank(list);
    expect(list.map((one) => one.id)).toEqual(["b", "a"]);
  });
});

describe("filters", () => {
  it("lists one chip for each kind that has an item, in the designs' order, with its count", () => {
    const items = [make("1", { kind: "main" }), make("2", { kind: "approval" }), make("3", { kind: "approval" }), make("4", { kind: "stopped" })];
    expect(viewOf(items).filters).toEqual([
      { kind: "approval", label: "Approvals", count: 2 },
      { kind: "main", label: "Main", count: 1 },
      { kind: "stopped", label: "Stopped", count: 1 },
    ]);
    expect(only(items, "approval")).toHaveLength(2);
    expect(only(items, "all")).toHaveLength(4);
  });
});
