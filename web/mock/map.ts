/**
 * The mock's Crew map reads: `GET /api/crew/flow/map`, `/messages` and `/facts`. They answer in the Crew scenarios (`crew`,
 * `crew-<state>`) and nowhere else, so the default scenario keeps every Crew route at 404 as the service does with Crew Preview off.
 * Reads only.
 *
 * Scenarios: `crew` (the lead and three workers), `crew-map-empty` (the lead alone), `crew-map-one`, `crew-map-many` (twelve
 * workers), `crew-map-deep` (a Claude worker, which reports its subagents), `crew-map-problems` (a failed worker, a quiet one and one
 * that may be stuck), `crew-map-offline` (Codex not installed), `crew-map-signedout` (Codex signed out), `crew-map-acp` (OpenCode,
 * which is not steered mid-turn), `crew-map-truncated`, `crew-map-no-messages`, `crew-map-many-messages` (more than one page), `crew-map-scrubbed` (members whose names the service scrubbed), `crew-map-hostile` (extra fields a service must never
 * send: a message body and a fact value, which the screens must never draw).
 */
import type { FlowEngineStatus } from "../src/generated/FlowEngineStatus.ts";
import type { FlowFact } from "../src/generated/FlowFact.ts";
import type { FlowMap } from "../src/generated/FlowMap.ts";
import type { FlowMessage } from "../src/generated/FlowMessage.ts";
import type { FlowNode } from "../src/generated/FlowNode.ts";
import type { MockResponse } from "./api.ts";
import { MOCK_CREW } from "./flow.ts";

import { HOSTILE } from "./hostile.ts";


const NOW = Date.parse("2026-11-07T15:00:00Z");
const at = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

const lead = (): FlowNode => ({ member: "lead", role: "lead", engine: "claude", model: "Opus 5.5", effort: "medium", quiet: "active", engine_status: "ready", subagents: "reported", outcome: "Fix login redirect", lease: [], read_only: true, unread: 0, created_at: at(134), updated_at: at(1) });
const worker = (member: string, engine: string, model: string, state: string, outcome: string, extra: Partial<FlowNode> = {}): FlowNode => ({
  member,
  role: "worker",
  task: `task-${member}`,
  engine,
  model,
  effort: "medium",
  state,
  quiet: "active",
  engine_status: "ready",
  subagents: engine === "claude" ? "reported" : "not_reported",
  outcome,
  lease: ["auth/redirect.ts"],
  read_only: false,
  unread: 0,
  started_by: "lead",
  created_at: at(30),
  updated_at: at(2),
  ...extra,
});

const status = (engine: string, scenario: string): FlowEngineStatus => (engine === "codex" && scenario === "crew-map-offline" ? "not_installed" : engine === "codex" && scenario === "crew-map-signedout" ? "signed_out" : "ready");

function nodes(scenario: string): FlowNode[] {
  const codexState = scenario === "crew-map-offline" || scenario === "crew-map-signedout" ? "needs_person" : "verifying";
  const base = [
    lead(),
    worker("implement", "claude", "Sonnet 5.5", "running", "Use next in the auth redirect when it points inside the app", { unread: 1 }),
    worker("tests", "codex", "GPT-5.5", codexState, "Add tests for the redirect, including //evil.com", { lease: ["auth/*.test.ts"], engine_status: status("codex", scenario) }),
    worker("review", "opencode", "DeepSeek V4", "running", "Which target wins when both next and returnTo are set?", { lease: [], read_only: true }),
  ];
  switch (scenario) {
    case "crew-map-empty": return [lead()];
    case "crew-map-scrubbed": return [lead(), worker("[redacted]", "codex", "GPT-5.5", "running", "Work whose member name was scrubbed"), worker("Withheld.", "opencode", "DeepSeek V4", "running", "Work whose member name was withheld"), base[2]!];
    case "crew-map-one": return [lead(), base[2]!];
    case "crew-map-many": return [lead(), ...Array.from({ length: 12 }, (_, i) => worker(`worker-${i + 1}`, ["claude", "codex", "opencode"][i % 3]!, "Sonnet 5.5", i % 4 === 0 ? "passed" : "running", `Task ${i + 1}`))];
    case "crew-map-deep": return [lead(), base[1]!];
    case "crew-map-acp": return [lead(), base[3]!];
    case "crew-map-problems":
      return [lead(), worker("implement", "claude", "Sonnet 5.5", "failed", "Typecheck failed twice, same error"), worker("tests", "codex", "GPT-5.5", "running", "Add tests for the redirect", { quiet: "quiet", lease: ["auth/*.test.ts"] }), worker("docs", "opencode", "DeepSeek V4", "running", "Update the docs", { quiet: "may_be_stuck", read_only: true, lease: [] })];
    default: return base;
  }
}

const sample = (scenario: string): FlowMessage[] => {
  if (scenario === "crew-map-no-messages" || scenario === "crew-map-empty") return [];
  if (scenario === "crew-map-many-messages") return Array.from({ length: 60 }, (_, i) => ({ id: `m${i}`, at: at(i), from: "implement", to: "tests", kind: "fact", label: "a fact", delivery: "live" as const }));
  const m = (i: number, minutes: number, from: string, to: string, kind: string, label: string, delivery: FlowMessage["delivery"]): FlowMessage => ({ id: `m${i}`, at: at(minutes), from, to, kind, label, delivery });
  return [
    m(1, 36, "implement", "tests", "fact", "a fact", "live"),
    m(2, 30, "lead", "review", "handoff", "a handoff", "next_turn"),
    m(3, 22, "tests", "implement", "blocker", "a blocker", "live"),
    m(4, 14, "review", "lead", "question", "a question", "next_turn"),
    m(5, 9, "lead", "all", "answer", "an answer", null),
    m(6, 4, "implement", "lead", "note", "a message", "held"),
    m(7, 3, "tests", "lead", "handoff", "a handoff", "tool_result"),
    m(8, 2, "lead", "tests", "fact", "a fact", "failed"),
  ];
};

const facts = (scenario: string): FlowFact[] =>
  scenario === "crew-map-empty" ? [] : [
    { key: "contract", class: "contract", author: "implement", rev: 2, at: at(36), verified: true },
    { key: "decision", class: "decision", author: "lead", rev: 1, at: at(20), verified: false },
    { key: "note", class: "note", author: "review", rev: 1, at: at(5), verified: false },
  ];

const missing = (what: string): MockResponse => ({ status: 404, body: { code: "not_found", message: `${what} not found` } });

/** The answer for a map read, or null when the route is not one of these (or Crew is off) and the other Crew mocks answer. */
export function mapMock(scenario: string, method: string, path: string, query: URLSearchParams): MockResponse | null {
  if (method !== "GET" || !(scenario === "crew" || scenario.startsWith("crew-"))) return null;
  if (path !== "/api/crew/flow/map" && path !== "/api/crew/flow/messages" && path !== "/api/crew/flow/facts") return null;
  const crew = query.get("crew");
  if (!crew) return { status: 422, body: { code: "invalid", message: "crew is required" } };
  if (crew !== MOCK_CREW) return missing("crew");
  const hostile = scenario === "crew-map-hostile";
  if (path === "/api/crew/flow/map") {
    const list = nodes(scenario);
    const engines = [...new Set(list.map((node) => node.engine))].map((engine) => ({ engine, status: status(engine, scenario) }));
    const body: FlowMap = { crew, nodes: list, edges: [{ from: "implement", to: "tests", kind: "fact", label: "a fact", count: 1, delivery: "live" }], engines, truncated: scenario === "crew-map-truncated" };
    return { status: 200, body: hostile ? { ...body, nodes: list.map((node) => ({ ...node, thread: HOSTILE.thread, worktree: HOSTILE.worktree, tokens: HOSTILE.tokens, level: HOSTILE.level })) } : body };
  }
  if (path === "/api/crew/flow/messages") {
    const member = query.get("member");
    const all = sample(scenario).filter((row) => !member || row.from === member || row.to === member || row.to === "all");
    const limit = Number(query.get("limit") ?? 50);
    const offset = Number(query.get("offset") ?? 0);
    const end = Math.min(offset + limit, all.length);
    const page = all.slice(offset, end).map((row) => (hostile ? { ...row, body: HOSTILE.body, text: HOSTILE.text } : row));
    return { status: 200, body: { messages: page, total: all.length, next_offset: end < all.length ? end : null } };
  }
  const list = facts(scenario).map((row) => (hostile ? { ...row, value: HOSTILE.value } : row));
  return { status: 200, body: { facts: list, total: list.length, truncated: false } };
}
