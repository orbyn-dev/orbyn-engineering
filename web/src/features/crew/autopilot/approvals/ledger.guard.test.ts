// How a guarded command's ledger rows become log rows, and what the Routing rows read: the stored shapes, with the optional parts absent.
import { describe, expect, it } from "vitest";
import type { AutopilotLogRow } from "../../../../generated/AutopilotLogRow";
import { withHolds } from "./ApprovalsTab";
import { guardBody, learn, rowOf, type Exit, type GuardBody } from "./ledger";
import type { ApprovalRow } from "./model";
import { routingRow, samplesOf } from "./routing";

const GUARD = { category: "database", target: "prod-db-1", matched_by: "psql: DROP", list: "none" };
const row = (id: number, kind: string, over: Partial<AutopilotLogRow> = {}): AutopilotLogRow => ({ id, task_id: "t1", crew_id: "c1", kind, at: "2026-11-07T14:05:00", paths: [], needs_second_engine: false, body: {}, ...over });
const read = (rows: AutopilotLogRow[]) => {
  const exits = new Map<number, Exit>();
  const holds = new Map<string, { guard: GuardBody; at: string }>();
  learn(rows, exits, holds);
  return { exits, holds, rows: rows.flatMap((one) => rowOf(one, [], [], { exits, holds }) ?? []) };
};

describe("a guarded command in the ledger", () => {
  it("reads the guard of a row, and a row without one, or with a target that is not text, has none", () => {
    expect(guardBody(row(1, "allowed", { body: { guard: GUARD } }))).toEqual({ category: "database", target: "prod-db-1", matchedBy: "psql: DROP", list: "none" });
    expect(guardBody(row(1, "allowed", { body: { guard: { category: "git" } } }))).toBeNull();
    expect(guardBody(row(1, "allowed", { body: { guard: { target: 7 } } }))).toBeNull();
    expect(guardBody(row(1, "allowed"))).toBeNull();
  });

  it("gives a command the person approved the exit the later row says, a number, `unknown`, or none yet", () => {
    const allowed = row(10, "allowed", { decided_by: "person", target: "psql -c x", body: { hold: "h", guard: GUARD } });
    const exit = (code: number | string) => row(11, "command_exit", { body: { row: 10, exit_code: code, guard: GUARD } });
    expect(read([exit(1), allowed]).rows[0]?.guard).toMatchObject({ exit: 1, exitUnknown: false });
    expect(read([exit("unknown"), allowed]).rows[0]?.guard).toMatchObject({ exit: null, exitUnknown: true });
    expect(read([allowed]).rows[0]?.guard).toMatchObject({ exit: null, exitUnknown: false });
    // An exit that names another row changes nothing here.
    expect(read([row(12, "command_exit", { body: { row: 99, exit_code: 3 } }), allowed]).rows[0]?.guard?.exit).toBeNull();
  });

  it("is approved by you when the person decided, by two keys otherwise, and the command is the title", () => {
    const base = { action_kind: "command", target: 'psql -c "TRUNCATE t"\nsecond', body: { guard: GUARD } };
    const person = read([row(1, "allowed", { decided_by: "person", ...base })]).rows[0];
    expect(person).toMatchObject({ result: "approved_you", hard: "guard", second: { state: "you" }, title: 'psql -c "TRUNCATE t"', block: base.target });
    const keys = read([row(2, "allowed", { decided_by: "two_keys", lead: { approve: true, reason: "ok" }, second: { engine: "codex", approve: true, reason: "yes" }, ...base })]).rows[0];
    expect(keys).toMatchObject({ result: "approved_two_keys", hard: "guard_two_keys", second: { state: "yes", by: { engine: "codex" } } });
  });

  it("is denied by the second key for a disagreement, once, and has no second key when none was signed in", () => {
    const keys = { lead: { approve: true, reason: "ok" }, second: { engine: "codex", approve: false, reason: "no" }, decided_by: "two_keys", body: { guard: GUARD } };
    expect(read([row(3, "disagreement", keys), row(4, "denied", keys)]).rows.map((r) => r.result)).toEqual(["denied_second_key"]);
    expect(read([row(5, "denied", { decided_by: "two_keys", needs_second_engine: true, lead: { approve: true, reason: "ok" }, body: { guard: GUARD } })]).rows[0]?.result).toBe("no_second_key");
    // A command with no guard keeps the words it had.
    expect(read([row(6, "disagreement", { ...keys, body: {} })]).rows[0]?.result).toBe("keys_disagree");
  });

  it("leaves out the denial the guard writes for a hold: the hold is the request", () => {
    expect(read([row(7, "denied", { decided_by: "guard", tier: "guard", body: { guard: GUARD } })]).rows).toEqual([]);
  });
});

describe("a hold and the ledger", () => {
  const hold = (id: string, command: string): ApprovalRow => ({ id, time: "15:02", at: null, title: command, asker: null, taskId: "t1", branch: null, risk: null, notes: [], facts: [], block: command, note: null, needsYou: null, mode: null, tier: null, hard: "guard", rule: null, lead: { state: "not_asked" }, leadBy: null, second: { state: "yours" }, result: "held_you", decidedInMs: null, guard: null, held: { id, state: "held", reason: "", readable: true, since: "15:02", decidedAt: null, byYou: false } });
  const denied = (command: string) => read([row(9, "denied", { decided_by: "two_keys", needs_second_engine: true, action_kind: "command", target: command, body: { guard: GUARD } })]).rows;

  it("takes the guard facts of the held row that names it", () => {
    const { rows, holds } = read([row(8, "held", { body: { hold: "h1", guard: GUARD } })]);
    const [one] = withHolds([hold("h1", "psql -c x")], { rows, holds });
    expect(one?.guard).toMatchObject({ target: "prod-db-1", kind: "remote database", at: "14:05" });
  });

  it("is drawn once when the command was also denied for want of a second key: the denied row takes the hold", () => {
    const loaded = { rows: denied("psql -c x"), holds: new Map() };
    const joined = withHolds([hold("h1", "psql -c x")], loaded);
    expect(joined).toHaveLength(1);
    expect(joined[0]).toMatchObject({ result: "no_second_key", held: { id: "h1" } });
  });

  it("keeps both when it is another command, another task or a hold already decided", () => {
    const loaded = { rows: denied("psql -c x"), holds: new Map() };
    expect(withHolds([hold("h2", "psql -c y")], loaded)).toHaveLength(2);
    expect(withHolds([{ ...hold("h3", "psql -c x"), taskId: "t2" }], loaded)).toHaveLength(2);
    const decided = hold("h4", "psql -c x");
    expect(withHolds([{ ...decided, held: decided.held && { ...decided.held, state: "approved" } }], loaded)).toHaveLength(2);
  });
});

describe("a routing row", () => {
  const body = { commit: "a1c4e7d9f0b2", blob: "5be90f2c1d3a", classes: [{ name: "auth-paths", pattern: "**/auth/**", samples: ["a", "b", "c", "d", "e"] }, { name: "docs", pattern: "docs/**", samples: ["a", "x"] }], blocked: [{ engine: "codex", rule: "auth-paths", reason: "r" }], chosen: "claude" };

  it("reads the classes, what a rule blocked, the engine chosen and the file's version", () => {
    expect(routingRow(row(1, "route_class", { body }))).toMatchObject({ result: "routed", chosen: "claude", commit: "a1c4e7d9f0b2", blocked: [{ engine: "codex" }], problem: null });
  });

  it("is `none` when no engine was chosen, and keeps the problem of an invalid file", () => {
    expect(routingRow(row(2, "route_class", { body: { ...body, chosen: null, problem: "Rule 3 has no engines listed." } }))).toMatchObject({ result: "none", chosen: null, problem: "Rule 3 has no engines listed." });
  });

  it("reads a row whose detail was withheld, with every part absent, and a row of another kind not at all", () => {
    expect(routingRow(row(3, "route_class", { body: { withheld: "secret" } }))).toMatchObject({ classes: [], blocked: [], chosen: null, commit: null, blob: null, result: "none" });
    expect(routingRow(row(4, "allowed"))).toBeNull();
  });

  it("shows three sample paths, each once, and counts the rest", () => {
    const one = routingRow(row(1, "route_class", { body }));
    if (!one) throw new Error("no row");
    expect(samplesOf(one)).toEqual({ shown: ["a", "b", "c"], more: 3 });
  });
});
