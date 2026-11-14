/**
 * The mock's Flow reads (`/api/crew/flow/*`): the crews that have work, a crew's tasks with the ten cells of each, and one task
 * zoomed. They answer in the Crew scenarios (`crew`, `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route
 * at 404 as the service does with Crew Preview off. The sample tasks exercise every state the real data can have: running, needs
 * you, failed after loops, merged and waiting for review, skipped stages, queued and stopped. Reads only.
 *
 * Scenarios: `crew` (two crews), `crew-empty` (no crews), `crew-flow-empty` (a crew with no tasks), `crew-flow-truncated`
 * (the service's cap was hit), `crew-flow-many` (more tasks than one page), `crew-flow-leak` (a service that wrongly sends a gate reason
 * for a task that is not over, which the screen must not show).
 */
import type { FlowCell } from "../src/generated/FlowCell.ts";
import type { FlowCellState } from "../src/generated/FlowCellState.ts";
import type { FlowCrewList } from "../src/generated/FlowCrewList.ts";
import type { FlowCounts } from "../src/generated/FlowCounts.ts";
import type { FlowFilter } from "../src/generated/FlowFilter.ts";
import type { FlowStage } from "../src/generated/FlowStage.ts";
import type { FlowTaskDetail } from "../src/generated/FlowTaskDetail.ts";
import type { FlowTaskList } from "../src/generated/FlowTaskList.ts";
import type { FlowTaskRow } from "../src/generated/FlowTaskRow.ts";
import type { MockResponse } from "./api.ts";

export const MOCK_CREW = "crew-redirect-0001";
export const MOCK_CREW_TWO = "crew-docs-0002";

const STAGES: FlowStage[] = ["brief", "route", "build", "checks", "code", "security", "finding", "merge", "main", "you"];

/** A line of ten cells: every stage is `pending` unless the list says otherwise. */
function line(states: Partial<Record<FlowStage, FlowCellState | [FlowCellState, Partial<FlowCell>]>>): FlowCell[] {
  return STAGES.map((stage) => {
    const given = states[stage];
    const [state, extra] = Array.isArray(given) ? given : [given ?? "pending", {}];
    return { stage, state, ...extra };
  });
}

interface Sample {
  row: FlowTaskRow;
  detail: Omit<FlowTaskDetail, "row" | "truncated">;
}

const row = (id: string, name: string, kind: string, engine: string | null, state: string, round: number, cells: FlowCell[], at: string): FlowTaskRow => ({
  id,
  name,
  kind,
  engine,
  owner: "Lead",
  state,
  round,
  cells,
  created_at: `2026-11-07T${at}:00Z`,
  updated_at: `2026-11-07T${at}:30Z`,
});

const brief = { purpose: "Use next when it points inside the app", files: ["auth/redirect.ts", "auth/redirect.test.ts"], budget_turns: 40, budget_minutes: 30 };

function samples(): Sample[] {
  return [
    {
      row: row("task-redirect", "Use next in the auth redirect", "implement", "claude", "verifying", 1, line({ brief: "done", route: "done", build: "done", checks: ["running", { passed: 2, of: 3 }], code: ["running", { lanes: 3 }], security: ["running", { lanes: 1 }] }), "14:02"),
      detail: {
        brief,
        route: { engine: "claude", model: "Sonnet 5.5", tier: "builder" },
        lanes: [
          { lane: "Correctness", group: "code", task: "task-redirect-r1", round: 1, state: "passed", engine: "codex", model: "GPT-5.5", findings: 0, confirmed: 0, unconfirmed: 0 },
          { lane: "Performance", group: "code", task: "task-redirect-r2", round: 1, state: "running", engine: "opencode", model: "DeepSeek V4", findings: 0, confirmed: 0, unconfirmed: 0 },
          { lane: "Docs", group: "code", task: "task-redirect-r3", round: 1, state: "passed", engine: "opencode", model: "DeepSeek V4", findings: 1, confirmed: 0, unconfirmed: 1 },
          { lane: "security", group: "security", task: "task-redirect-r4", round: 1, state: "running", engine: "codex", model: "GPT-5.5", findings: 0, confirmed: 0, unconfirmed: 0 },
        ],
        findings: [{ id: "f-1", lane: "Docs", round: 1, severity: "low", path: "docs/login.md", line: 12, claim: "The example uses //evil.com", check: "unconfirmed", status: "open", blocks: false, fix_task: null }],
        gates: [{ stage: "checks", decision: "passed", decided_by: "code", lane: null, reason: null, at: "2026-11-07T14:10:00Z" }],
        rounds: [{ n: 1, task: "task-redirect", state: "verifying" }],
        merge: null,
        watch: null,
        you: null,
      },
    },
    {
      row: row("task-needs", "Project icon picker", "implement", "codex", "needs_person", 1, line({ brief: "done", route: "done", build: "needs_you" }), "13:40"),
      detail: { brief: { ...brief, purpose: "Let a person pick a project icon" }, route: { engine: "codex", model: "Terra", tier: "builder" }, lanes: [], findings: [], gates: [], rounds: [{ n: 1, task: "task-needs", state: "needs_person" }], merge: null, watch: null, you: { review_state: "waiting", due_unix: null, needs_person: "The worker asked which icon set to use." } },
    },
    {
      row: row("task-failed", "Rename the legacy returnTo helper", "implement", "claude", "failed", 3, line({ brief: "done", route: "done", build: "failed", checks: ["failed", { passed: 2, of: 3 }] }), "13:10"),
      detail: { brief, route: { engine: "claude", model: "Sonnet 5.5", tier: "builder" }, lanes: [], findings: [], gates: [{ stage: "checks", decision: "failed", decided_by: "code", lane: null, reason: null, at: "2026-11-07T13:20:00Z" }], rounds: [{ n: 1, task: "task-failed", state: "failed" }, { n: 2, task: "task-failed-fix1", state: "failed" }, { n: 3, task: "task-failed-fix2", state: "failed" }], merge: null, watch: null, you: null },
    },
    {
      row: row("task-merged", "Update the login docs", "docs", "codex", "passed", 1, line({ brief: "done", route: "done", build: "done", checks: ["done", { passed: 2, of: 2 }], code: ["done", { lanes: 2 }], security: "skipped", finding: "done", merge: "done", main: "done", you: "review_waiting" }), "12:30"),
      detail: {
        brief: { ...brief, purpose: "Say how the redirect works", files: ["docs/login.md"] },
        route: { engine: "codex", model: "Terra", tier: "builder" },
        lanes: [{ lane: "Docs", group: "code", task: "task-merged-r1", round: 1, state: "passed", engine: "codex", model: "GPT-5.5", findings: 0, confirmed: 0, unconfirmed: 0 }],
        findings: [],
        gates: [
          { stage: "checks", decision: "passed", decided_by: "code", lane: null, reason: "2 of 2 checks ran on the commit and passed", at: "2026-11-07T12:40:00Z" },
          { stage: "merge", decision: "merged", decided_by: "code", lane: null, reason: "Merged by Crew <b>docs</b>", at: "2026-11-07T12:50:00Z" },
        ],
        rounds: [{ n: 1, task: "task-merged", state: "passed" }],
        merge: { state: "merged", risk: "low", reasons: ["docs only"], mode: "full" },
        watch: "green",
        you: { review_state: "waiting", due_unix: Date.parse("2026-11-08T12:50:00Z") / 1000, needs_person: null },
      },
    },
    {
      row: row("task-queued", "Move the CHANGELOG entry", "docs", null, "briefed", 1, line({ brief: "done" }), "12:00"),
      detail: { brief: { ...brief, purpose: "Move the entry", files: ["CHANGELOG.md"] }, route: null, lanes: [], findings: [], gates: [], rounds: [{ n: 1, task: "task-queued", state: "briefed" }], merge: null, watch: null, you: null },
    },
    {
      row: row("task-stopped", "Add a rate limit to the login route", "implement", "codex", "stopped", 2, line({ brief: "done", route: "done", build: "done", checks: ["done", { passed: 5, of: 5 }], code: ["done", { lanes: 3 }], security: ["failed", { lanes: 2 }], finding: "failed" }), "11:20"),
      detail: {
        brief: { ...brief, purpose: "Add a rate limit", files: ["auth/login.ts"] },
        route: { engine: "codex", model: "Sol", tier: "lead" },
        lanes: [{ lane: "security", group: "security", task: "task-stopped-r1", round: 2, state: "passed", engine: "claude", model: "Opus 5.5", findings: 2, confirmed: 2, unconfirmed: 0 }],
        findings: [{ id: "f-2", lane: "security", round: 2, severity: "high", path: "auth/login.ts", line: 41, claim: "The limit resets on every request", check: "confirmed", status: "open", blocks: true, fix_task: "task-stopped-fix" }],
        gates: [{ stage: "finding", decision: "blocks", decided_by: "confirmed_finding", lane: "security", reason: "A confirmed finding still blocks", at: "2026-11-07T11:50:00Z" }],
        rounds: [{ n: 1, task: "task-stopped", state: "failed" }, { n: 2, task: "task-stopped-fix", state: "stopped" }],
        merge: null,
        watch: null,
        you: null,
      },
    },
  ];
}

/** Sixty plain rows for the paging state. */
function many(): Sample[] {
  return Array.from({ length: 60 }, (_, at) => ({
    row: row(`task-m${at}`, `Sample task ${at + 1}`, "implement", at % 2 === 0 ? "claude" : "codex", "passed", 1, line({ brief: "done", route: "done", build: "done", checks: ["done", { passed: 3, of: 3 }] }), "10:00"),
    detail: { brief, route: null, lanes: [], findings: [], gates: [], rounds: [{ n: 1, task: `task-m${at}`, state: "passed" }], merge: null, watch: null, you: null },
  }));
}

const has = (cells: FlowCell[], pred: (cell: FlowCell) => boolean) => cells.some(pred);
const inFilter = (task: FlowTaskRow, filter: FlowFilter): boolean => {
  switch (filter) {
    case "all": return true;
    case "needs_you": return has(task.cells, (c) => c.state === "needs_you");
    case "reviewing": return has(task.cells, (c) => (c.stage === "code" || c.stage === "security") && c.state === "running");
    case "failed": return has(task.cells, (c) => c.state === "failed");
    case "passed": return task.state === "passed";
  }
};

const missing = (what: string): MockResponse => ({ status: 404, body: { code: "not_found", message: `${what} not found` } });
const invalid = (message: string): MockResponse => ({ status: 422, body: { code: "invalid", message } });

/** The answer for a Flow read, or null when the route is not one of these (or Crew is off) and the other Crew mocks answer. */
export function flowMock(scenario: string, method: string, path: string, query: URLSearchParams): MockResponse | null {
  if (method !== "GET" || !path.startsWith("/api/crew/flow/") || !(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  const truncated = scenario === "crew-flow-truncated";
  const all = scenario === "crew-flow-many" ? many() : scenario === "crew-flow-empty" ? [] : samples();
  if (path === "/api/crew/flow/crews") {
    if (scenario === "crew-empty") return { status: 200, body: { crews: [], total: 0, next_offset: null } satisfies FlowCrewList };
    const crews = [
      { id: MOCK_CREW, label: MOCK_CREW.slice(0, 12), tasks: all.length, finished: all.filter((one) => one.row.state === "passed").length, updated_at: "2026-11-07T14:02:30Z" },
      ...(scenario === "crew" ? [{ id: MOCK_CREW_TWO, label: MOCK_CREW_TWO.slice(0, 12), tasks: 1, finished: 0, updated_at: "2026-11-06T09:00:00Z" }] : []),
    ];
    return { status: 200, body: { crews, total: crews.length, next_offset: null } satisfies FlowCrewList };
  }
  if (path === "/api/crew/flow/tasks") {
    const crew = query.get("crew");
    if (!crew) return invalid("crew is required");
    const filter = (query.get("filter") ?? "all") as FlowFilter;
    if (!["all", "needs_you", "reviewing", "failed", "passed"].includes(filter)) return invalid("that filter is not one of the list");
    const mine = crew === MOCK_CREW ? all : crew === MOCK_CREW_TWO ? [samples()[4]!] : null;
    if (!mine) return missing("crew");
    const counts: FlowCounts = { all: mine.length, needs_you: 0, reviewing: 0, failed: 0, passed: 0 };
    for (const one of mine) {
      counts.needs_you += Number(inFilter(one.row, "needs_you"));
      counts.reviewing += Number(inFilter(one.row, "reviewing"));
      counts.failed += Number(inFilter(one.row, "failed"));
      counts.passed += Number(inFilter(one.row, "passed"));
    }
    const needle = (query.get("q") ?? "").trim().toLowerCase();
    const engine = query.get("engine");
    const matched = mine
      .map((one) => one.row)
      .filter((one) => inFilter(one, filter) && (!engine || one.engine === engine) && (!needle || `${one.name} ${one.kind} ${one.engine ?? ""}`.toLowerCase().includes(needle)));
    // A row that needs the person rises to the top; the order inside each group stays as listed.
    matched.sort((a, b) => Number(inFilter(b, "needs_you")) - Number(inFilter(a, "needs_you")));
    const limit = Number(query.get("limit") ?? 50);
    const offset = Number(query.get("offset") ?? 0);
    const end = Math.min(offset + limit, matched.length);
    const body: FlowTaskList = { crew, counts, tasks: matched.slice(offset, end), total: matched.length, next_offset: end < matched.length ? end : null, truncated };
    return { status: 200, body };
  }
  const id = /^\/api\/crew\/flow\/tasks\/([^/]+)$/.exec(path)?.[1];
  if (id) {
    const found = [...samples(), ...many()].find((one) => one.row.id === decodeURIComponent(id));
    if (!found) return missing("task");
    const leaked = scenario === "crew-flow-leak" ? found.detail.gates.map((gate) => ({ ...gate, reason: "A reason for a task that is not over" })) : found.detail.gates;
    const body: FlowTaskDetail = { row: found.row, ...found.detail, gates: leaked, truncated: false };
    return { status: 200, body };
  }
  return null;
}
