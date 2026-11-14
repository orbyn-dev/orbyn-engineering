/**
 * The mock's Taskboard read (`GET /api/crew/flow/board`): a crew's work tasks as seven columns. It answers in the Crew scenarios
 * (`crew`, `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route at 404 as the service does with Crew
 * Preview off. Reads only.
 *
 * Scenarios: `crew` (the design's sample: one card in each of the first five columns, three passed, two stopped or cancelled),
 * `crew-board-many` (twelve running tasks, so "+N more" pages), `crew-board-empty` (no task), `crew-board-truncated` (the service's
 * cap was hit).
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
    card("needs_you", "b-needs", "research", "Which target wins when both next and returnTo are set?", "opencode", "DeepSeek V4", "medium", "needs_person", 1, { reason: "Asked you a question: <b>which target wins</b>?" }),
    card("passed", "b-pass1", "docs", "Fix the typo in the README", "codex", "GPT-5.5", "low", "passed", 120, { can_rerun: true }),
    card("passed", "b-pass2", "implement", "Move session helpers to auth/session.ts", "claude", "Sonnet 5.5", "medium", "passed", 600, { can_rerun: false }),
    card("passed", "b-pass3", "test", "Cover the login route", "codex", "GPT-5.5", "high", "passed", 2000),
    card("stopped_cancelled", "b-stop1", "implement", "Add a rate limit to the login route", "codex", "GPT-5.5", "medium", "stopped", 300, { stop_reason: "repeated_failure", can_rerun: true }),
    card("stopped_cancelled", "b-stop2", "docs", "Move the CHANGELOG entry", "claude", "Sonnet 5.5", "low", "cancelled", 900),
  ];
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

/** The answer for a board read, or null when the route is not the board's (or Crew is off) and the other Crew mocks answer. */
export function boardMock(scenario: string, method: string, path: string, query: URLSearchParams): MockResponse | null {
  if (method !== "GET" || path !== "/api/crew/flow/board" || !(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  const crew = query.get("crew");
  if (!crew) return invalid("crew is required");
  if (crew !== MOCK_CREW && crew !== MOCK_CREW_TWO) return { status: 404, body: { code: "not_found", message: "crew not found" } };
  const all = scenario === "crew-board-empty" || crew === MOCK_CREW_TWO ? [] : scenario === "crew-board-many" ? many() : sample();
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
