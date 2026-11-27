/**
 * The mock's Taskboard read (`GET /api/crew/flow/board`): a crew's work tasks as seven columns. It answers in the Crew scenarios
 * (`crew`, `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route at 404 as the service does with Crew
 * Preview off. It also answers the person's cancel, re-run-checks and retry-elsewhere writes.
 *
 * Scenarios: `crew` (the design's sample: one card in each of the first five columns, three passed, two stopped or cancelled),
 * `crew-board-many` (twelve running tasks, so "+N more" pages), `crew-board-empty` (no task), `crew-board-truncated` (the service's
 * cap was hit), `crew-board-working` (the sample's retry in Running and checks in Verifying, with Needs you empty),
 * `crew-board-needs-kinds` (the three needs-you kinds the page gives its own amber sentence), and `crew-could-not-stop*` (the
 * sample's verifying task moved to Needs you as the cancelled card whose worker could not be stopped, kc2-close-lead-could-not-stop).
 */
import type { FlowBoard } from "../src/generated/FlowBoard.ts";
import type { FlowCard } from "../src/generated/FlowCard.ts";
import type { FlowColumn } from "../src/generated/FlowColumn.ts";
import type { FlowColumnId } from "../src/generated/FlowColumnId.ts";
import type { FlowFacet } from "../src/generated/FlowFacet.ts";
import type { MockResponse } from "./api.ts";
import { MOCK_CREW, MOCK_CREW_TWO } from "./flow.ts";

type Row = FlowCard & { column: FlowColumnId };

const NOW = Date.parse("2026-11-07T15:00:00Z");
const at = (minutesAgo: number) => new Date(NOW - minutesAgo * 60_000).toISOString();

const card = (column: FlowColumnId, id: string, kind: string, name: string, engine: string | null, model: string | null, effort: string | null, state: string, minutes: number, extra: Partial<FlowCard> = {}): Row => ({
  column,
  id,
  name,
  kind,
  owner: "Lead",
  engine,
  model,
  effort,
  state,
  updated_at: at(minutes),
  could_not_start: false,
  can_cancel: !["passed", "stopped", "cancelled"].includes(state),
  can_rerun: state === "passed",
  ...extra,
});

function sample(): Row[] {
  return [
    card("queued", "b-queued", "docs", "Update the login docs for the new redirect rule", null, null, null, "briefed", 1),
    card("routed", "b-routed", "implement", "Rename returnTo to next in the session code", "claude", "Sonnet 5.5", "low", "routed", 0),
    card("running", "b-running", "implement", "Use next in the auth redirect when it points inside the app", "claude", "Sonnet 5.5", "medium", "running", 0),
    card("verifying", "b-verifying", "test", "Add tests for the redirect, including //evil.com", "codex", "GPT-5.5", "medium", "verifying", 0),
    card("needs_you", "b-needs", "research", "Which target wins when both next and returnTo are set?", "opencode", "DeepSeek V4", "medium", "needs_person", 1, { needs_you: "task_needs_person", reason: "Which target wins when both next and returnTo are set?" }),
    card("passed", "b-pass1", "docs", "Fix the typo in the README", "codex", "GPT-5.5", "low", "passed", 120, { can_rerun: true, checks: { passed: 4, of: 4 } }),
    card("passed", "b-pass2", "implement", "Move session helpers to auth/session.ts", "claude", "Sonnet 5.5", "medium", "passed", 600, { can_rerun: false, checks: { passed: 3, of: 3 } }),
    card("passed", "b-pass3", "test", "Cover the login route", "codex", "GPT-5.5", "high", "passed", 2000),
    card("stopped_cancelled", "b-stop1", "implement", "Add a rate limit to the login route", "codex", "GPT-5.5", "medium", "stopped", 300, { stop_reason: "repeated_failure", can_rerun: true }),
    card("stopped_cancelled", "b-stop2", "docs", "Move the CHANGELOG entry", "claude", "Sonnet 5.5", "low", "cancelled", 900),
  ];
}

/**
 * `crew-board-working`: the sample's live workers, a retry in Running and checks in Verifying, with nothing in Needs you (the
 * working frame, needs-one-source-working-board).
 */
function workingBoard(): Row[] {
  return sample()
    .filter((row) => row.column !== "needs_you")
    .map((row): Row => (row.id === "b-running" ? { ...row, working: "retry" } : row.id === "b-verifying" ? { ...row, working: "verifying" } : row));
}

/** `crew-board-needs-kinds`: one card for each needs-you kind the Taskboard gives its own amber sentence. */
function needsKinds(): Row[] {
  return [
    card("needs_you", "b-question", "research", "Which target wins when both next and returnTo are set?", "opencode", "DeepSeek V4", "medium", "needs_person", 1, { needs_you: "task_needs_person", reason: "Which target wins when both next and returnTo are set?" }),
    card("needs_you", "b-merge", "implement", "Rotate the signing key", "claude", "Sonnet 5.5", "high", "passed", 4, { needs_you: "merge_waiting", reason: "A merge needs your decision." }),
    card("needs_you", "b-manual", "test", "Check the redirect by hand on staging", "codex", "GPT-5.5", "medium", "needs_person", 9, { needs_you: "manual_check", reason: "A check only a person can make is waiting." }),
  ];
}

/**
 * The `crew-could-not-stop*` scenarios: the sample, with the verifying task whose lead close could not stop its worker moved to
 * Needs you, cancelled and final, carrying the service's own line (kc2-close-lead-could-not-stop).
 */
function couldNotStop(): Row[] {
  return sample().map((row) => {
    if (row.id !== "b-verifying") return row;
    const moved: Row = {
      ...row,
      column: "needs_you",
      state: "cancelled",
      can_cancel: false,
      needs_you: "could_not_stop",
      reason: "The worker could not be stopped. Stop it again.",
    };
    return moved;
  });
}

/** `crew-could-not-start`: two implement tasks whose Codex workers were refused at their start, beside one task that really waits. */
function couldNotStart(): Row[] {
  const refused = "Crew workers on Codex are not available yet.";
  return [
    card("needs_you", "b-refused1", "implement", "Rename returnTo to next in the session code", "codex", "GPT-5.5", "medium", "needs_person", 2, { could_not_start: true, reason: refused }),
    card("needs_you", "b-refused2", "implement", "Use next in the auth redirect when it points inside the app", "codex", "GPT-5.5", "medium", "needs_person", 3, { could_not_start: true, reason: refused }),
    card("needs_you", "b-needs", "research", "Which target wins when both next and returnTo are set?", "opencode", "DeepSeek V4", "medium", "needs_person", 5, { reason: "No check this version can run decided the task, so a person decides." }),
  ];
}

/**
 * `crew-could-not-start-disk`: the same two refused tasks, with the disk guard's structured answer:
 * the code and both numbers, and a reason that deliberately is not the disk sentence.
 */
function couldNotStartDisk() {
  const free = 3_400_000_000;
  return [
    card("needs_you", "b-refused1", "implement", "Rename returnTo to next in the session code", "codex", "GPT-5.5", "medium", "needs_person", 2, { could_not_start: true, reason: "A different sentence." }),
    card("needs_you", "b-refused2", "implement", "Use next in the auth redirect when it points inside the app", "codex", "GPT-5.5", "medium", "needs_person", 3, { could_not_start: true, reason: "" }),
  ].map((entry) => ({ ...entry, code: "disk_low", free_bytes: free, floor_gb: 5 }));
}

function many(): Row[] {
  return Array.from({ length: 12 }, (_, i) => card("running", `b-m${i}`, i % 2 ? "test" : "implement", `Running task ${i + 1}`, i % 3 ? "claude" : "codex", "Sonnet 5.5", "medium", "running", i));
}

const IDS: FlowColumnId[] = ["queued", "routed", "running", "verifying", "needs_you", "passed", "stopped_cancelled"];
const facet = (rows: Row[], pick: (row: Row) => string | null | undefined): FlowFacet[] => {
  const counts = new Map<string, number>();
  for (const row of rows) {
    const name = pick(row);
    if (name) counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  return [...counts].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
};

const invalid = (message: string): MockResponse => ({ status: 422, body: { code: "invalid", message } });

const WRITE = /^\/api\/crew\/tasks\/([^/]+)\/(cancel|rerun-checks|retry-elsewhere)$/;
const KNOWN = /^(b-|task-)/;
const fail = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });

/**
 * The person's two writes, for the Taskboard and the Flow zoom: `POST /api/crew/tasks/{id}/cancel` and `/rerun-checks`. They answer in
 * the Crew scenarios only, and the answers are counts and states, never a command. Scenarios: `crew-write-already` (a cancel of a task
 * already cancelled), `crew-write-ended` (409 task_ended), `crew-write-engine` (502, the task is not cancelled),
 * `crew-write-engine-cancelled` (502, the task is cancelled but its worker may still run), `crew-write-busy` (a re-run answers 409
 * task_busy); any other Crew scenario succeeds. A slow answer is made in a test, by holding the request.
 */
function writeMock(scenario: string, path: string): MockResponse | null {
  const found = WRITE.exec(path);
  if (!found) return null;
  const id = decodeURIComponent(found[1]!);
  if (!KNOWN.test(id)) return fail(404, "not_found", "task not found");
  if (found[2] === "cancel") {
    if (scenario === "crew-write-ended") return fail(409, "task_ended", "This task already ended.");
    if (scenario === "crew-write-engine") return fail(502, "engine_unavailable", "The task was not cancelled. Try again.");
    if (scenario === "crew-write-engine-cancelled") return fail(502, "engine_unavailable", "The task is cancelled, but its worker could not be stopped. Try again.");
    return { status: 200, body: { task: id, state: "cancelled", already: scenario === "crew-write-already" } };
  }
  if (found[2] === "retry-elsewhere") {
    if (scenario === "crew-could-not-start-no-engine") return fail(409, "task_busy", "No other engine is set up to run crew workers. Turn one on in Settings, or dismiss this task.");
    return { status: 200, body: { task: id, lead_told: scenario !== "crew-could-not-start-lead-away" } };
  }
  if (scenario === "crew-write-busy") return fail(409, "task_busy", "Another re-run is running. Try again in 7 seconds.");
  return { status: 200, body: { task: id, state: "passed", total: 4, ran: 3, passed: 2, refused: 1, skipped: 1, gate: "fail", refused_checks: ["check-2"] } };
}

/** The answer for a board read, or null when the route is not the board's (or Crew is off) and the other Crew mocks answer. */
export function boardMock(scenario: string, method: string, path: string, query: URLSearchParams): MockResponse | null {
  if (!(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  if (method === "POST") return writeMock(scenario, path);
  if (method !== "GET" || path !== "/api/crew/flow/board") return null;
  const crew = query.get("crew");
  if (!crew) return invalid("crew is required");
  if (crew !== MOCK_CREW && crew !== MOCK_CREW_TWO) return { status: 404, body: { code: "not_found", message: "crew not found" } };
  const all = scenario === "crew-board-empty" || crew === MOCK_CREW_TWO ? []
    : scenario === "crew-board-many" ? many()
    : scenario === "crew-board-working" ? workingBoard()
    : scenario === "crew-board-needs-kinds" ? needsKinds()
    : scenario === "crew-could-not-start-disk" ? couldNotStartDisk()
    : scenario.startsWith("crew-could-not-start") ? couldNotStart()
    : scenario.startsWith("crew-could-not-stop") ? couldNotStop()
    : sample();
  const engine = query.get("engine");
  const kind = query.get("kind");
  const only = query.get("column") as FlowColumnId | null;
  if (only && !IDS.includes(only)) return invalid("that column is not on the board");
  const perColumn = Number(query.get("per_column") ?? 8);
  const limit = Number(query.get("limit") ?? 50);
  const offset = Number(query.get("offset") ?? 0);
  const kept = all.filter((row) => (!engine || row.engine === engine) && (!kind || row.kind === kind));
  const columns: FlowColumn[] = IDS.filter((id) => !only || id === only).map((id) => {
    const rows = kept.filter((row) => row.column === id);
    // Newest first; in Needs you an urgent card would lead (none is drawn urgent in the sample).
    rows.sort((a, b) => b.updated_at.localeCompare(a.updated_at) || a.id.localeCompare(b.id));
    const start = only ? Math.min(offset, rows.length) : 0;
    const end = Math.min(start + (only ? limit : perColumn), rows.length);
    return { column: id, count: rows.length, cards: rows.slice(start, end).map(({ column: _column, ...rest }) => rest), next_offset: end < rows.length ? end : null };
  });
  const body: FlowBoard = { crew, total: all.length, engines: facet(all, (row) => row.engine), kinds: facet(all, (row) => row.kind), columns, truncated: scenario === "crew-board-truncated" };
  return { status: 200, body };
}
