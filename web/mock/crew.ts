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
 * The guard (slice 4i): `crew-guard` (a guarded command each way in the ledger: both keys, the person once with an exit status, a second key that
 * said no, no second vendor, and the held row each live hold writes), `crew-guard-unknown` (the person approved, and no exit status came: `unknown`),
 * `crew-guard-empty` (this device lists no target). The lists answer in every Crew scenario. Path classes: `crew-classes`, `crew-classes-invalid`
 * (the file has problems and every engine is blocked), `crew-classes-note` (the checkout's copy differs from the committed one). Routing:
 * `crew-route-blocked`, `crew-route-invalid`, `crew-route-none` (the ledger's `route_class` rows and `GET /api/crew/route/task-redirect`).
 * As the service does, `classes` needs `thread`, `classes/test` takes 1 to 10 paths and says what is wrong with each, and an unknown task is 404.
 */
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
import type { CrewClassEngine } from "../src/generated/CrewClassEngine.ts";
import type { CrewClassRule } from "../src/generated/CrewClassRule.ts";
import type { CrewClassesView } from "../src/generated/CrewClassesView.ts";
import type { CrewClassTest } from "../src/generated/CrewClassTest.ts";
import type { CrewClassTestPath } from "../src/generated/CrewClassTestPath.ts";
import type { CrewRouteClassRow } from "../src/generated/CrewRouteClassRow.ts";
import type { CrewRouteView } from "../src/generated/CrewRouteView.ts";
import type { GuardTargets } from "../src/generated/GuardTargets.ts";
import type { RouteRejection } from "../src/generated/RouteRejection.ts";
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

const TRUNCATE = 'psql "$PROD_DATABASE_URL" -c "TRUNCATE audit_log"';
const AUDIT = 'psql "$PROD_DATABASE_URL" -c "DELETE FROM audit_log WHERE created_at < now() - interval \'90 days\'"';
const DROP = 'psql "$PROD_DATABASE_URL" -c "ALTER TABLE users DROP COLUMN legacy_id"';
const SESSIONS = 'psql "$PROD_DATABASE_URL" -c "DELETE FROM sessions WHERE expires_at < now()"';
const guard = (target = "prod-db-1", list = "prod") => ({ category: "database", target, matched_by: "psql: DROP", list });

/**
 * What a guarded command leaves in the ledger, newest first: the exit status a later `command_exit` row gives the row that let the command through
 * (`body.row`), both keys' yes, a second key that said no (stored twice, as a disagreement and the denial it caused), no second vendor signed in,
 * the person's one approval, and the `held` row a live hold writes. A denial under On or Off (`decided_by: guard`) is the hold's own record.
 */
const guardRows = (unknown: boolean): AutopilotLogRow[] => [
  row(3003, "16:03", "command_exit", { task_id: "task-redirect", decided_by: "autopilot", body: { hold: "hold-audit", row: 3002, exit_code: unknown ? "unknown" : 0, guard: guard() } }),
  row(3002, "16:02", "allowed", { decided_by: "person", action_kind: "command", target: AUDIT, body: { hold: "hold-audit", guard: guard() } }),
  row(3001, "15:02", "held", { decided_by: "autopilot", action_kind: "command", target: ORDERS, body: { hold: "hold-orders", guard: guard() } }),
  row(3000, "15:02", "denied", { decided_by: "guard", tier: "guard", action_kind: "command", target: ORDERS, body: { guard: guard() } }),
  row(2991, "14:21", "command_exit", { decided_by: "autopilot", body: { hold: null, row: 2990, exit_code: 0, guard: guard() } }),
  row(2990, "14:05", "allowed", {
    decided_by: "two_keys",
    tier: "guard",
    action_kind: "command",
    target: SESSIONS,
    lead: { approve: true, reason: "expired sessions, and the brief names them." },
    second: { engine: "codex", approve: true, reason: "the filter matches rows past expires_at." },
    body: { guard: guard() },
  }),
  row(2985, "14:20", "disagreement", {
    decided_by: "two_keys",
    tier: "guard",
    action_kind: "command",
    target: DROP,
    lead: { approve: true, reason: "The column isn’t used in the diff." },
    second: { engine: "codex", approve: false, reason: "a report still reads users.legacy_id." },
    body: { guard: guard() },
  }),
  row(2986, "14:20", "denied", {
    decided_by: "two_keys",
    tier: "guard",
    action_kind: "command",
    target: DROP,
    lead: { approve: true, reason: "The column isn’t used in the diff." },
    second: { engine: "codex", approve: false, reason: "a report still reads users.legacy_id." },
    body: { guard: guard() },
  }),
  row(2980, "13:58", "denied", { decided_by: "two_keys", tier: "guard", action_kind: "command", target: TRUNCATE, needs_second_engine: true, lead: { approve: true, reason: "It is a cleanup in the brief." }, body: { guard: guard() } }),
];

/** The guard's lists on this device, as `GET /api/crew/guard/targets` answers. */
const TARGETS: GuardTargets = {
  max_entries: 200,
  items: [
    { id: "g1", list: "prod", pattern: "prod-db-1", kind: "host", note: "Remote database", added_at: "2026-11-01T09:00:00" },
    { id: "g2", list: "prod", pattern: "orbyn-assets-prod", kind: "bucket", note: "Storage bucket", added_at: "2026-11-01T09:05:00" },
    { id: "g3", list: "safe", pattern: "staging-db.internal", kind: "host", note: "A throwaway staging database", added_at: "2026-11-02T09:00:00" },
  ],
};

const COMMIT = "a1c4e7d9f0b2c3d4e5f60718293a4b5c6d7e8f90";
const BLOB = "5be90f2c1d3a4b5c6d7e8f9012345678abcdef01";
const ENGINES: Record<string, CrewClassEngine> = {
  claude: { id: "claude", name: "Claude Code", on_this_machine: true },
  codex: { id: "codex", name: "Codex", on_this_machine: true },
  opencode: { id: "opencode", name: "OpenCode", on_this_machine: true },
  cursor: { id: "cursor", name: "Cursor", on_this_machine: false },
};
const RULES: CrewClassRule[] = [
  { name: "auth-paths", paths: ["**/auth/**", "**/keys/**"], engines: [ENGINES.claude as CrewClassEngine], reason: "Sign-in code stays on one vendor." },
  { name: "billing", paths: ["**/billing/**"], engines: [ENGINES.codex as CrewClassEngine], reason: "Money code stays on one vendor." },
  { name: "docs", paths: ["docs/**", "*.md"], engines: [ENGINES.claude, ENGINES.codex, ENGINES.opencode] as CrewClassEngine[], reason: "Anyone may edit the docs." },
];
const PROBLEMS = ["Rule 4 (keys): the path pattern **/keys/[ isn’t valid: the bracket isn’t closed.", "Rule 4 (keys): no engine is chosen.", "Rule 5 (billing): the name billing is already used by rule 2."];

/** The file's globs as the service reads them: `/`-separated, `*` inside a part, a part that is `**` for any number of parts, case ignored. */
export function globMatch(pattern: string, path: string): boolean {
  const walk = (p: string[], s: string[]): boolean => {
    if (p.length === 0) return s.length === 0;
    const [head, ...rest] = p;
    if (head === "**") return walk(rest, s) || (s.length > 0 && walk(p, s.slice(1)));
    if (s.length === 0) return false;
    const re = new RegExp(`^${(head as string).split("*").map((part) => part.replace(/[.+^${}()|\\]/g, "\\$&")).join(".*")}$`, "i");
    return re.test(s[0] as string) && walk(rest, s.slice(1));
  };
  return walk(pattern.split("/"), path.split("/"));
}

/** What the service says is wrong with a path that is not a repository path, or null. */
const pathProblem = (path: string): string | null =>
  path.includes("\\") ? "has a backslash" : path.startsWith("/") ? "starts with /" : path.split("/").some((part) => part === "" || part === "." || part === "..") ? "has an empty, . or .. part" : null;

function testPath(path: string, rules: CrewClassRule[]): CrewClassTestPath {
  const problem = pathProblem(path);
  if (problem) return { path, valid: false, problem: `This path ${problem}.`, classes: [], allowed_engines: null, route_card: "Not a repository path, so nothing was checked." };
  const hits = rules.flatMap((rule) => rule.paths.filter((pattern) => globMatch(pattern, path)).slice(0, 1).map((pattern) => ({ rule, pattern })));
  let allowed: CrewClassEngine[] | null = null;
  for (const { rule } of hits) allowed = allowed === null ? rule.engines : allowed.filter((one) => rule.engines.some((other) => other.id === one.id));
  const names = hits.map((hit) => hit.rule.name);
  const route_card = !hits.length ? "No rule covers this path: any signed-in engine may take it." : allowed && allowed.length === 0 ? "No engine is allowed for these paths" : `Allowed by ${names.join(" and ")}: ${allowed?.map((one) => one.name).join(", ")}.`;
  return { path, valid: true, problem: null, classes: hits.map((hit) => ({ name: hit.rule.name, pattern: hit.pattern })), allowed_engines: allowed, route_card };
}

const reject = (engine: string, text: string, why: RouteRejection["why"] = "path_class"): RouteRejection => ({ engine, why, text });

/** The `route_class` row a routing writes, as `GET /api/crew/route/{task}` lists it. */
function routeRow(kind: "blocked" | "invalid" | "none"): CrewRouteClassRow {
  const base = { id: "7001", at: "2026-11-07T14:20:00", commit: COMMIT, blob: BLOB };
  if (kind === "invalid") {
    const blocked = ["claude", "codex", "opencode"].map((engine) => ({ engine, rule: "crew.json invalid", reason: "The file isn’t valid." }));
    return { ...base, problem: "Rule 3 has no engines listed.", classes: [], blocked, chosen: null, rejected: blocked.map((one) => reject(one.engine, "Blocked by rule: the file isn’t valid")), message: ".orbyn/crew.json isn’t valid, so no engine was started" };
  }
  if (kind === "none") {
    const classes = [{ name: "auth-paths", pattern: "**/auth/**", samples: ["auth/redirect.ts"] }, { name: "billing", pattern: "**/billing/**", samples: ["billing/invoice.ts"] }];
    const blocked = [
      { engine: "claude", rule: "billing", reason: "Money code stays on one vendor." },
      { engine: "codex", rule: "auth-paths", reason: "Sign-in code stays on one vendor." },
      { engine: "opencode", rule: "auth-paths", reason: "Sign-in code stays on one vendor." },
      { engine: "opencode", rule: "billing", reason: "Money code stays on one vendor." },
    ];
    return { ...base, classes, blocked, chosen: null, rejected: [reject("claude", "Blocked by rule: billing — allowed engines: Codex"), reject("codex", "Blocked by rule: auth-paths — allowed engines: Claude"), reject("opencode", "Blocked by both rules: auth-paths and billing"), reject("cursor", "Not signed in.", "not_signed_in")], message: "No engine is allowed for these paths." };
  }
  const classes = [{ name: "auth-paths", pattern: "**/auth/**", samples: ["auth/redirect.ts", "auth/redirect.test.ts", "auth/session.ts"] }];
  return { ...base, classes, blocked: [{ engine: "codex", rule: "auth-paths", reason: "Sign-in code stays on one vendor." }, { engine: "opencode", rule: "auth-paths", reason: "Sign-in code stays on one vendor." }], chosen: "claude", rejected: [] };
}

function routeView(kind: "blocked" | "invalid" | "none"): CrewRouteView {
  const row = routeRow(kind);
  if (kind === "blocked") {
    return {
      task: "task-redirect",
      state: "running",
      rows: [row],
      decision: {
        v: 0, id: "sha256:0", crew_id: "crew-redirect", author: "orbyn", created_at: "2026-11-07T14:20:00", run_id: "run-1", brief: "sha256:1", engine: "claude", model: "Sonnet 5.5", effort: "medium", tier: "medium", role: "build", assigned_by: "router",
        reasons: [{ code: "path_class", text: "The rule auth-paths allows claude for the paths this task touches, and it is signed in." }],
        rejected: [reject("codex", "Blocked by rule: auth-paths, allowed engines: Claude. Sign-in code stays on one vendor."), reject("opencode", "Blocked by rule: auth-paths, allowed engines: Claude. Sign-in code stays on one vendor."), reject("cursor", "Not signed in.", "not_signed_in")],
        scores: [],
      } as unknown as NonNullable<CrewRouteView["decision"]>,
    };
  }
  return { task: "task-redirect", state: "needs_you", rows: [row], refusal: { message: row.message ?? "", rejected: row.rejected } };
}

/** The two holds `crew-guard` has live: one for the person, and the command no second vendor could back. */
const guardHolds = (): AutopilotHeld[] => [
  { id: "hold-orders", task_id: "task-redirect", crew_id: "crew-redirect", kind: "command", command: ORDERS, reason: "This command can destroy data on a production-linked target. Autopilot does not approve it in this mode; it is held for you.", state: "held", readable: true, age_secs: 120, expires_in_secs: 480 },
  { id: "hold-audit-2", task_id: "task-redirect", crew_id: "crew-redirect", kind: "command", command: TRUNCATE, reason: "A second key is missing.", state: "held", readable: true, age_secs: 300, expires_in_secs: 300 },
];

export function createCrewMock() {
  let guardsEmpty = false;
  let classes: "valid" | "invalid" | "note" = "valid";
  let route: "blocked" | "invalid" | "none" | null = null;
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

  const modes = (): AutopilotModes => ({ confirmed, effective_default: mode, entries, truncated: false });

  function reset(scenario: string) {
    on = scenario === "crew" || scenario.startsWith("crew-");
    mode = "full";
    confirmed = scenario !== "crew-first-run";
    entries = scenario === "crew-per-crew"
      ? [{ scope: "crew", id: "login-redirect", mode: "on", updated_at: "2026-11-07T10:00:00" }, { scope: "crew", id: "dependency-bump", mode: "full", updated_at: "2026-11-06T10:00:00" }]
      : [];
    young = scenario === "crew-young";
    guardsEmpty = scenario === "crew-guard-empty";
    classes = scenario === "crew-classes-invalid" ? "invalid" : scenario === "crew-classes-note" ? "note" : "valid";
    route = scenario === "crew-route-blocked" ? "blocked" : scenario === "crew-route-invalid" ? "invalid" : scenario === "crew-route-none" ? "none" : null;
    heldScenario = scenario.startsWith("crew-held") ? scenario : "";
    held = scenario === "crew-guard" ? guardHolds() : heldScenario ? holds() : [];
    if (scenario === "crew-guard") mode = "on";
    failPaging = scenario === "crew-many-fails";
    all = scenario === "crew-empty" ? [] : scenario === "crew-many" || scenario === "crew-many-fails" ? many() : scenario === "crew-one-vendor" ? sample().map((one) => (one.id === 2 ? { ...one, kind: "denied", decided_by: "two_keys", second: null, needs_second_engine: true } : one)) : sample();
    if (scenario === "crew-guard" || scenario === "crew-guard-unknown") all = [...guardRows(scenario === "crew-guard-unknown"), ...all];
    if (route) all = [row(7001, "14:20", "route_class", { body: routeRow(route) as unknown as AutopilotLogRow["body"] }), ...all];
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
    if (method === "GET" && head === "guard" && id === "targets") return { status: 200, body: guardsEmpty ? { ...TARGETS, items: [] } : TARGETS };
    if (method === "GET" && head === "classes" && !id) {
      const thread = query.get("thread");
      if (!thread) return { status: 400, body: { code: "invalid", message: "thread is required" } };
      const bad = classes === "invalid";
      const view: CrewClassesView = {
        thread,
        commit: COMMIT,
        blob: BLOB,
        worktree: "feat/crew-map",
        built_in: "Everything else",
        rules: bad ? [] : RULES,
        problems: bad ? PROBLEMS : [],
        notes: [],
        working_copy_differs: classes === "note",
        working_copy_note: classes === "note" ? "This checkout's copy of the file differs from the committed one. An edit is not in force until it is committed." : null,
      };
      return { status: 200, body: view };
    }
    if (method === "GET" && head === "classes" && id === "test") {
      const thread = query.get("thread");
      if (!thread) return { status: 400, body: { code: "invalid", message: "thread is required" } };
      // `path` repeated, or one per line.
      const paths = query.getAll("path").flatMap((one) => one.split("\n")).filter((one) => one !== "");
      if (paths.length < 1 || paths.length > 10) return { status: 400, body: { code: "invalid", message: "give 1 to 10 paths" } };
      const bad = classes === "invalid";
      const test: CrewClassTest = { thread, commit: COMMIT, blob: BLOB, problems: bad ? PROBLEMS : [], paths: paths.map((one) => (bad ? { ...testPath(one, []), allowed_engines: [], route_card: "crew.json is invalid, so every engine is blocked." } : testPath(one, RULES))) };
      return { status: 200, body: test };
    }
    if (method === "GET" && head === "route" && id) {
      if (id !== "task-redirect") return { status: 404, body: { code: "not_found", message: "task not found" } };
      return { status: 200, body: routeView(route ?? "blocked") };
    }
    return { status: 404, body: { code: "not_found", message: `No mock for ${method} ${path}.` } };
  }

  return { reset, handle };
}
