/**
 * The mock's Crew Preview: the routes of `orbynd` (`/api/crew/...`) with the generated wire shapes, answering with
 * ledger rows that read like the Crew designs' samples. The default scenario has Crew off, so every Crew route
 * answers 404 as the service does; scenarios named `crew` or `crew-<state>` turn it on: crew, crew-empty,
 * crew-many, crew-one-vendor, crew-tripwire, crew-injected, crew-per-crew, crew-first-run, crew-young.
 * Background crews have no route, so the mock has none either.
 *
 * Held commands: `crew-held` (approving once is the desktop app's native dialog, which a spec fakes in the page, so the route only
 * answers as the service does), `crew-held-expired` (the first hold has timed out when it is answered: 410), `crew-held-race` (it
 * was denied from another window: 409), `crew-held-hardrule` (the always-denied list refuses it at approval time: 403, and the hold is
 * closed as denied) and `crew-held-unconfirmed` (the service did not see the desktop shell's confirmation: 403 `presence_required`).
 * Three commands are held in every one of them.
 *
 * Autopilot by area (slice 4j): `crew-areas` (every area follows its ceiling, one is held at On by its own ceiling, one has too few merges),
 * `crew-areas-tripped`, `crew-areas-ceiling-off` (web/auth demoted, then its ceiling lowered to Off), `crew-areas-recovering`,
 * `crew-areas-criteria-met`, `crew-areas-restored` (web/auth came back, with the trail's `restored` row) and `crew-areas-lowdata`. The override
 * and follow routes change the area as the service does and answer 409 `stale` when the record the person saw is not the current one;
 * `crew-areas-stale` makes the first choice stale. The ceiling is per repository, as in the service: no area has its own.
 */
import type { AutopilotAreaEvent } from "../src/generated/AutopilotAreaEvent.ts";
import type { AutopilotAreaRow } from "../src/generated/AutopilotAreaRow.ts";
import type { AutopilotAreas } from "../src/generated/AutopilotAreas.ts";
import type { AutopilotThresholds } from "../src/generated/AutopilotThresholds.ts";
import type { AutopilotHeld } from "../src/generated/AutopilotHeld.ts";
import type { AutopilotHeldList } from "../src/generated/AutopilotHeldList.ts";
import type { AutopilotGuards } from "../src/generated/AutopilotGuards.ts";
import type { AutopilotLog } from "../src/generated/AutopilotLog.ts";
import type { AutopilotLogRow } from "../src/generated/AutopilotLogRow.ts";
import type { AutopilotMode } from "../src/generated/AutopilotMode.ts";
import type { AutopilotModeEntry } from "../src/generated/AutopilotModeEntry.ts";
import type { AutopilotModes } from "../src/generated/AutopilotModes.ts";
import type { AutopilotTripwires } from "../src/generated/AutopilotTripwires.ts";
import type { AutopilotTrust } from "../src/generated/AutopilotTrust.ts";
import type { MockResponse } from "./api.ts";

const row = (id: number, time: string, kind: string, over: Partial<AutopilotLogRow>): AutopilotLogRow => ({
  id,
  task_id: "task-redirect",
  crew_id: "crew-redirect",
  kind,
  at: `2026-11-07T${time}:00`,
  paths: [],
  mode: "full",
  needs_second_engine: false,
  body: {},
  ...over,
});

const sample = (): AutopilotLogRow[] => [
  row(6, "14:45", "person", { rule: "N9", action_kind: "publish", target: "the package to the registry", decided_by: "hard_rule" }),
  row(5, "14:37", "disagreement", {
    tier: "medium",
    action_kind: "run",
    target: "node db/gen-migration.js",
    lead: { approve: true, reason: "It matches the brief’s migration test." },
    second: { engine: "opencode", approve: false, reason: "it writes to a table outside the brief." },
  }),
  // A disagreement is stored twice: its own row, then the denial it caused.
  row(55, "14:37", "denied", {
    decided_by: "two_keys",
    tier: "medium",
    action_kind: "run",
    target: "node db/gen-migration.js",
    lead: { approve: true, reason: "It matches the brief’s migration test." },
    second: { engine: "opencode", approve: false, reason: "it writes to a table outside the brief." },
  }),
  row(4, "14:33", "denied", { decided_by: "lead", tier: "low", action_kind: "run", target: "scripts/gen.sh", lead: { approve: false, reason: "The script isn’t in the brief, and the worker didn’t say what it changes." } }),
  row(3, "14:26", "denied", { decided_by: "hard_rule", rule: "N4", action_kind: "write", target: "../shared/config.json" }),
  row(2, "14:19", "allowed", {
    tier: "medium",
    decided_by: "two_keys",
    action_kind: "install",
    target: "zod@3.24.1",
    lead: { approve: true, reason: "It’s pinned to one version and the brief asks for schema tests." },
    second: { engine: "opencode", approve: true, reason: "It’s pinned." },
  }),
  row(1, "14:12", "allowed", { tier: "always_allowed", decided_by: "tier", action_kind: "read", target: "auth/redirect.ts" }),
];

/** 214 requests, newest first, each after a merge row that the log leaves out. */
const many = (): AutopilotLogRow[] => {
  const base = sample().filter((one) => one.id !== 55);
  return Array.from({ length: 214 }, (_, i) => {
    const one = base[i % base.length] as AutopilotLogRow;
    const minutes = 15 * 60 + 2 + (213 - i) * 3;
    const time = `${String(Math.floor(minutes / 60) % 24).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
    return [row(5000 - i, time, "merged", { task_id: "task-merge" }), { ...one, id: 1000 - i, at: `2026-11-07T${time}:00` }];
  }).flat();
};

const GUARDS: AutopilotGuards = {
  always_denied: [
    ["N1", "changing account, device or sign-in state"],
    ["N2", "reading or sending secrets and credentials"],
    ["N3", "network access to a host the repository has not allowed"],
    ["N4", "writing or deleting outside the worktree"],
    ["N5", "force-push, history rewrite or any push to main"],
    ["N6", "changing branch protection, permissions or settings"],
    ["N7", "editing answer keys, check files or .orbyn/crew.json"],
    ["N8", "production deploys"],
    ["N9", "publishing or messaging outside the repository"],
    ["N10", "payments"],
  ].map(([id, rule]) => ({ id: id as string, rule: rule as string })),
  in_a_row_limit: 3,
  in_a_run_limit: 20,
  review_windows: [],
  large_files: 20,
  large_lines: 800,
  high_revert_permille: 200,
  min_sample: 10,
  window_days: 60,
  default_mode: "full",
  lead_engine: "claude-code",
  second_key_engines: ["opencode", "codex"],
};

const rate = (merges: number, reverts: number) => ({ merges, reverts, revert_permille: merges ? Math.round((reverts * 1000) / merges) : 0, counts: merges >= 10 });

const trust = (young: boolean): AutopilotTrust =>
  young
    ? { window_days: 60, min_sample: 10, high_revert_permille: 200, total: rate(5, 0), areas: [], engines: [{ name: "claude-code", rate: rate(3, 0) }, { name: "codex", rate: rate(2, 0) }], autonomy_by_area: false, truncated: false }
    : {
        window_days: 60,
        min_sample: 10,
        high_revert_permille: 200,
        total: rate(53, 6),
        engines: [{ name: "claude-code", rate: rate(31, 2) }, { name: "codex", rate: rate(18, 1) }, { name: "opencode", rate: rate(4, 0) }],
        areas: [{ name: "docs/", rate: rate(31, 0) }, { name: "auth/", rate: rate(14, 2) }, { name: "keys/", rate: rate(11, 3) }],
        autonomy_by_area: false,
        truncated: false,
      };

const ORDERS = 'psql "$PROD_DATABASE_URL" -c "DELETE FROM orders WHERE is_test = true"';

const holds = (): AutopilotHeld[] => [
  { id: "hold-orders", task_id: "task-redirect", crew_id: "crew-redirect", kind: "command", command: ORDERS, reason: "The command is not in the brief.", state: "held", readable: true, age_secs: 120, expires_in_secs: 480 },
  { id: "hold-bucket", task_id: "task-redirect", crew_id: "crew-redirect", kind: "command", command: "aws s3 rb s3://orbyn-assets-prod --force", reason: "The brief never mentions this bucket.", state: "held", readable: true, age_secs: 300, expires_in_secs: 300 },
  { id: "hold-sessions", task_id: "task-redirect", crew_id: "crew-redirect", kind: "command", command: 'psql "$PROD_DATABASE_URL" -c "DELETE FROM sessions WHERE expires_at < now()"', reason: "A second key is missing.", state: "held", readable: true, age_secs: 420, expires_in_secs: 180 },
];

/** The sample's clock, in the machine's own zone, so the dates the screens print do not move with the time zone. */
const at = (day: number, hour: number, minute: number) => Math.floor(new Date(2026, 10, day, hour, minute).getTime() / 1000);

const THRESHOLDS: AutopilotThresholds = { window_days: 14, min_merges: 5, trip_reverts: 2, recovery_days: 14, recovery_merges: 5, proposed: true };

const areaRow = (area: string, merges: number, reverts: number, over: Partial<AutopilotAreaRow> = {}): AutopilotAreaRow => ({
  area,
  area_key: area,
  ceiling: "full",
  now: "full",
  state: merges < 5 ? "not_enough_data" : "follows",
  line: merges < 5 ? `Not enough data: ${merges} of 5 merges in 14 days.` : `Follows the ceiling. ${reverts} of ${merges} merges reverted in 14 days.`,
  record: { merges, reverts, window_days: 14 },
  ...over,
});

const steady = (): AutopilotAreaRow[] => [
  areaRow("docs/guides", 12, 0),
  areaRow("web/app", 7, 0),
  areaRow("crates/orbyn-core", 6, 0),
  areaRow("crates/orbyn-keys", 6, 1),
  areaRow(".github/workflows", 3, 0),
];

const lowered = (state: "demoted" | "recovering" | "criteria_met", over: Partial<AutopilotAreaRow>): AutopilotAreaRow =>
  areaRow("web/auth", 9, 2, { now: "on", state, since: at(7, 15, 36), line: "Lowered after 2 of 9 merges were reverted in 14 days.", ...over });

function areaScenario(scenario: string): { ceiling: AutopilotAreas["ceiling"]; asOf: number; rows: AutopilotAreaRow[]; trail: AutopilotAreaEvent[] } {
  const rest = steady();
  const full = "full" as const;
  const web = (row: AutopilotAreaRow) => [row, ...rest];
  const restored: AutopilotAreaEvent = { id: 3, kind: "restored", from_mode: "on", to_mode: "full", decided_by: "breaker", cause: `restore:${at(7, 15, 34)}`, record: { merges: 6, reverts: 0, window_days: 14 }, at: at(21, 16, 10) };
  switch (scenario) {
    case "crew-areas-tripped":
      return { ceiling: full, asOf: at(7, 15, 36), rows: web(lowered("demoted", {})), trail: [] };
    case "crew-areas-stale":
      return { ceiling: full, asOf: at(7, 15, 36), rows: web(lowered("demoted", {})), trail: [] };
    case "crew-areas-ceiling-off":
      return {
        ceiling: "off",
        asOf: at(7, 17, 5),
        rows: web(lowered("demoted", {})).map((row) => ({ ...row, ceiling: "off", now: "off", ...(row.state === "follows" || row.state === "not_enough_data" ? { state: "ceiling_lower" as const } : {}) })),
        trail: [],
      };
    case "crew-areas-recovering":
      return { ceiling: full, asOf: at(14, 10, 0), rows: web(lowered("recovering", { record: { merges: 3, reverts: 0, window_days: 14 }, recovery: { clean_days: 7, days_needed: 14, merges: 3, merges_needed: 5 } })), trail: [] };
    case "crew-areas-criteria-met":
      return { ceiling: full, asOf: at(21, 15, 40), rows: web(lowered("criteria_met", { record: { merges: 6, reverts: 0, window_days: 14 }, recovery: { clean_days: 14, days_needed: 14, merges: 6, merges_needed: 5 } })), trail: [] };
    case "crew-areas-restored":
      return { ceiling: full, asOf: at(21, 16, 10), rows: web(areaRow("web/auth", 6, 0)), trail: [restored] };
    case "crew-areas-lowdata":
      return { ceiling: full, asOf: at(2, 9, 0), rows: [areaRow("web/auth", 3, 0), areaRow("crates/orbyn-core", 4, 0), areaRow("web/app", 2, 0), areaRow(".github/workflows", 3, 0)], trail: [] };
    default:
      return { ceiling: full, asOf: at(6, 17, 0), rows: web(areaRow("web/auth", 8, 1)), trail: [] };
  }
}

export function createCrewMock() {
  let held: AutopilotHeld[] = [];
  let heldScenario = "";
  let on = false;
  let failPaging = false;
  let mode: AutopilotMode = "full";
  let confirmed = true;
  let entries: AutopilotModeEntry[] = [];
  let all: AutopilotLogRow[] = [];
  let tripwire: AutopilotLogRow[] = [];
  let young = false;
  let areas = areaScenario("crew-areas");
  let staleOnce = false;

  const modes = (): AutopilotModes => ({ confirmed, effective_default: mode, entries, truncated: false });

  function reset(scenario: string) {
    on = scenario === "crew" || scenario.startsWith("crew-");
    mode = "full";
    confirmed = scenario !== "crew-first-run";
    entries = scenario === "crew-per-crew"
      ? [{ scope: "crew", id: "login-redirect", mode: "on", updated_at: "2026-11-07T10:00:00" }, { scope: "crew", id: "dependency-bump", mode: "full", updated_at: "2026-11-06T10:00:00" }]
      : [];
    young = scenario === "crew-young";
    areas = areaScenario(scenario);
    staleOnce = scenario === "crew-areas-stale";
    heldScenario = scenario.startsWith("crew-held") ? scenario : "";
    held = heldScenario ? holds() : [];
    failPaging = scenario === "crew-many-fails";
    all = scenario === "crew-empty" ? [] : scenario === "crew-many" || scenario === "crew-many-fails" ? many() : scenario === "crew-one-vendor" ? sample().map((one) => (one.id === 2 ? { ...one, kind: "denied", decided_by: "two_keys", second: null, needs_second_engine: true } : one)) : sample();
    const wire: Record<string, string | number> = scenario === "crew-injected" ? { wire: "injection", class: "instructions" } : { wire: "denials", in_a_row: 3, in_a_run: 3 };
    tripwire = scenario === "crew-tripwire" || scenario === "crew-injected" ? [row(90, "14:27", "tripwire", { task_id: "task-client", body: wire })] : [];
    if (tripwire.length) {
      all = [
        ...all,
        row(80, "14:26", "denied", { task_id: "task-client", decided_by: "hard_rule", rule: "N3", action_kind: "fetch", target: "https://cdn.example/spec" }),
        row(79, "14:24", "denied", { task_id: "task-client", decided_by: "lead", action_kind: "run", target: "scripts/gen.sh", lead: { approve: false, reason: "Not in the brief" } }),
        row(78, "14:21", "denied", { task_id: "task-client", decided_by: "hard_rule", rule: "N4", action_kind: "write", target: "../client/api.ts" }),
      ];
    }
  }

  function approvals(params: URLSearchParams): AutopilotLog {
    const offset = Number(params.get("offset") ?? 0);
    const limit = Number(params.get("limit") ?? 50);
    const kind = params.get("kind");
    const task = params.get("task");
    const list = all.filter((one) => (!kind || one.kind === kind) && (!task || one.task_id === task));
    const next = offset + limit < list.length ? offset + limit : null;
    return { rows: list.slice(offset, offset + limit), total: list.length, next_offset: next };
  }

  function areasRoute(method: string, tail: string | undefined, body: unknown, query: URLSearchParams): MockResponse {
    if (method === "GET" && !tail) {
      const out: AutopilotAreas = { project: query.get("project") ?? "", ceiling: areas.ceiling, as_of: areas.asOf, enforced: true, total: areas.rows.length, rows: areas.rows };
      return { status: 200, body: out };
    }
    if (method === "GET" && tail === "history") return { status: 200, body: { project: query.get("project") ?? "", area: query.get("area") ?? "", events: query.get("area") === "web/auth" ? areas.trail : [], truncated: false } };
    if (method !== "POST" || (tail !== "override" && tail !== "follow")) return { status: 404, body: { code: "not_found", message: "No such route." } };
    const asked = body as { area: string; choice?: string; acknowledged?: boolean; seen?: { merges: number; reverts: number; tripped_at?: number | null } };
    const row = areas.rows.find((one) => one.area_key === asked.area);
    if (!row) return { status: 404, body: { code: "not_found", message: "That area is not on this repository." } };
    const moved = staleOnce || (asked.seen !== undefined && asked.seen !== null && (asked.seen.merges !== row.record.merges || asked.seen.reverts !== row.record.reverts || (asked.seen.tripped_at ?? null) !== (row.since ?? null)));
    if (moved && tail === "override") {
      staleOnce = false;
      return { status: 409, body: { code: "stale", message: "The record changed since you looked. Read it again before choosing." } };
    }
    let next: AutopilotAreaRow;
    if (tail === "follow") {
      const kept = { ...row };
      delete kept.override;
      next = { ...kept, state: row.since && row.recovery ? "recovering" : row.since ? "demoted" : "follows", now: row.since ? (row.ceiling === "full" ? "on" : row.ceiling) : row.ceiling };
    } else if (asked.choice === "off") {
      next = { ...row, state: "overridden", now: "off", override: { choice: "off", at: areas.asOf }, line: "A person set this area to off." };
    } else if (asked.choice === "keep_ceiling" && asked.acknowledged && row.since) {
      next = { ...row, state: "overridden", now: row.ceiling, override: { choice: "keep_ceiling", at: areas.asOf }, line: "A person set this area to keep ceiling." };
    } else return { status: 400, body: { code: "invalid", message: "That choice is not allowed for this area." } };
    areas = { ...areas, rows: areas.rows.map((one) => (one.area_key === row.area_key ? next : one)) };
    return { status: 200, body: next };
  }

  function handle(method: string, path: string, body: unknown, query: URLSearchParams, confirm?: string): MockResponse {
    if (!on) return { status: 404, body: { code: "not_found", message: "Crew is not on." } };
    const seg = path.split("/").filter(Boolean); // api crew ...
    const [, , head, id] = seg;
    if (method === "GET" && head === "preview") return { status: 200, body: { enabled: true } };
    if (head === "approvals" && id === "held" && method === "GET") {
      const out: AutopilotHeldList = { items: held, hold_secs: 600, approval_secs: 600 };
      return { status: 200, body: out };
    }
    if (method === "POST" && head === "approvals" && id) {
      const how = seg[4];
      const item = held.find((one) => one.id === id);
      if (!item || !["approve-once", "deny", "dismiss"].includes(how ?? "")) return { status: 404, body: { code: "not_found", message: "held command not found" } };
      const set = (state: AutopilotHeld["state"]) => {
        held = held.map((one) => (one.id === id ? { ...one, state, expires_in_secs: state === "approved" ? 600 : 0 } : one));
        return { status: 200, body: held.find((one) => one.id === id) };
      };
      // The service checks the nonce in `x-orbyn-confirm` first; the mock only checks that one was sent.
      if (how === "approve-once" && (heldScenario === "crew-held-unconfirmed" || !confirm)) {
        return { status: 403, body: { code: "presence_required", message: "Approving needs the person's confirmation from the desktop app." } };
      }
      if (heldScenario === "crew-held-expired" && id === "hold-orders") {
        held = held.map((one) => (one.id === id ? { ...one, state: "expired", expires_in_secs: 0 } : one));
        return { status: 410, body: { code: "gone", message: "This command is no longer held: its time ran out or its task ended." } };
      }
      if (heldScenario === "crew-held-hardrule" && id === "hold-orders" && how === "approve-once") {
        set("denied");
        return { status: 403, body: { code: "policy_denied", message: "Denied by Autopilot (N4): Writing or deleting outside the worktree. No approval can change this." } };
      }
      if (heldScenario === "crew-held-race" && id === "hold-orders") {
        set("denied");
        return { status: 409, body: { code: "invalid_transition", message: "run cannot move from denied to decided" } };
      }
      if (item.state !== "held") return { status: 409, body: { code: "invalid_transition", message: `run cannot move from ${item.state} to decided` } };
      return set(how === "approve-once" ? "approved" : how === "deny" ? "denied" : "dismissed");
    }
    if (method === "GET" && head === "approvals" && failPaging && Number(query.get("offset") ?? 0) > 18) return { status: 500, body: { code: "internal", message: "The ledger could not be read." } };
    if (method === "GET" && head === "approvals") return { status: 200, body: approvals(query) };
    if (method === "GET" && head === "stops") {
      const out: AutopilotTripwires = { in_a_row_limit: 3, in_a_run_limit: 20, running: [], recent: tripwire };
      return { status: 200, body: out };
    }
    if (head === "autopilot" && id === "areas") return areasRoute(method, seg[4], body, query);
    if (method === "GET" && head === "autopilot" && id === "thresholds") return { status: 200, body: THRESHOLDS };
    if (method === "GET" && head === "autopilot" && !id) return { status: 200, body: modes() };
    if (method === "GET" && head === "autopilot" && id === "guards") return { status: 200, body: GUARDS };
    if (method === "PUT" && head === "autopilot" && id === "mode") {
      const change = body as { scope: "default" | "repo" | "crew"; id?: string; mode: AutopilotMode };
      if (change.scope === "default") mode = change.mode;
      entries = [{ scope: change.scope, id: change.id ?? null, mode: change.mode, updated_at: new Date().toISOString() }, ...entries.filter((one) => !(one.scope === change.scope && one.id === (change.id ?? null)))];
      confirmed = true;
      return { status: 200, body: modes() };
    }
    if (method === "POST" && head === "autopilot" && id === "confirm") {
      confirmed = true;
      return { status: 200, body: modes() };
    }
    if (method === "GET" && head === "trust") return { status: 200, body: trust(young) };
    return { status: 404, body: { code: "not_found", message: `No mock for ${method} ${path}.` } };
  }

  return { reset, handle };
}
