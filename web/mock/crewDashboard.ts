/**
 * MOCK of the Crew dashboard's reads, for tests and the dev-only Preview frames `#/preview/crew/dash-*` (Crew Designs, "23. Dashboard").
 * The app's own client is the real one in src/features/crew/dashboard/contract.ts, which has no `history` read at all: nothing here ships.
 * `history` answers the planned `GET /api/crew/dashboard` of docs/crew-dashboard.md on the sample the designs draw (orbyn-engineering:
 * 38 merges in 60 days, 27 of them without the person, 2 reverted), with the KC3 live row: the default repository has two waits and two
 * working workers, and every other repository has none. The `crew-dashboard-live-truncated` scenario leaves one running worker and
 * `truncated: true`; `crew-dashboard-live-none` answers as an older service with no `live` key at all. `trust`, `areas`, `thresholds` and
 * `areaHistory` answer from the same sample, so the page can be seen offline. Sample figures: nothing here was measured.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { AutopilotAreaHistory } from "../src/generated/AutopilotAreaHistory.ts";
import type { AutopilotAreaRow } from "../src/generated/AutopilotAreaRow.ts";
import type { AutopilotAreas } from "../src/generated/AutopilotAreas.ts";
import type { AutopilotThresholds } from "../src/generated/AutopilotThresholds.ts";
import type { AutopilotTrust } from "../src/generated/AutopilotTrust.ts";
import type { DashboardClient, DashboardHistory, HistoryMerge, Period } from "../src/features/crew/dashboard/contract.ts";
import type { MockResponse } from "./api.ts";

/** The projects the frames name. Ids are the names. */
export const SAMPLE_PROJECTS = [
  { id: "orbyn-engineering", name: "orbyn-engineering" },
  { id: "orbyn-planner", name: "orbyn-planner" },
  { id: "orbyn-engine", name: "orbyn-engine" },
  { id: "orbyn-docs", name: "orbyn-docs" },
] as const;

const NOW = 1_762_500_000;
const WEEK = 7 * 86_400;
const weeks = (rows: Array<[number, number, number]>) => rows.map(([merges, unaided, reverted], i) => ({ start: NOW - (rows.length - i) * WEEK, merges, unaided, reverted }));
const tokens = (amount: number) => ({ unit: "tokens", amount });

const BASE: Record<string, DashboardHistory> = {
  "orbyn-engineering": {
    project: "orbyn-engineering", days: 60, as_of: NOW, min_rate_sample: 10, min_median_sample: 5,
    merges: 38, unaided: 27, reverted: 2, reverted_auto: 1, reverted_by_person: 1,
    cycle: { median_s: 41 * 60, p90_s: 125 * 60, fastest_s: 6 * 60, slowest_s: 220 * 60 },
    weeks: weeks([[3, 2, 0], [5, 3, 0], [4, 3, 1], [6, 4, 0], [5, 4, 0], [4, 3, 0], [7, 5, 1], [4, 3, 0]]),
    engines: [
      { name: "claude", model: "Sonnet 5.5", merges: 21, unaided: 16, reverted: 1, cost: tokens(6_100_000) },
      { name: "codex", model: "GPT-5.5", merges: 12, unaided: 8, reverted: 1, cost: tokens(3_400_000) },
      { name: "opencode", model: "DeepSeek V4", merges: 5, unaided: 3, reverted: 0, cost: tokens(2_300_000) },
    ],
    areas: [
      { name: "docs/guides", merges: 14, unaided: 13, reverted: 0 },
      { name: "web/auth", merges: 15, unaided: 8, reverted: 2 },
      { name: "web/app", merges: 9, unaided: 6, reverted: 0 },
    ],
    reverts: [
      { title: "Rename the legacy returnTo helper", at: NOW - 25 * 86_400, by: "auto", certain: true },
      { title: "Rotate the signing key", at: NOW - 54 * 86_400, by: "person", certain: null },
    ],
    recent: [],
    live: {
      needs_you: [{ kind: "task_needs_person", count: 1 }, { kind: "merge_waiting", count: 1 }],
      working: [{ why: "verifying", count: 1 }, { why: "retry", count: 1 }],
      truncated: false,
    },
    truncated: false,
  },
  "orbyn-planner": {
    project: "orbyn-planner", days: 60, as_of: NOW, min_rate_sample: 10, min_median_sample: 5,
    merges: 5, unaided: 3, reverted: 0, reverted_auto: 0, reverted_by_person: 0,
    cycle: { median_s: 55 * 60, p90_s: 80 * 60, fastest_s: 20 * 60, slowest_s: 90 * 60 },
    weeks: weeks([[0, 0, 0], [1, 1, 0], [2, 1, 0], [2, 1, 0]]),
    engines: [
      { name: "claude", model: "Sonnet 5.5", merges: 3, unaided: 2, reverted: 0, cost: tokens(1_400_000) },
      { name: "codex", model: "GPT-5.5", merges: 2, unaided: 1, reverted: 0, cost: tokens(540_000) },
    ],
    areas: [{ name: "calendar", merges: 5, unaided: 3, reverted: 0 }],
    reverts: [],
    recent: [
      { title: "Update the calendar feed token", engine: "claude", at: NOW - 15 * 86_400, needed: "nothing", checks: "passed" },
      { title: "Fix the flaky E2E shard", engine: "codex", at: NOW - 16 * 86_400, needed: "answer", checks: "passed" },
      { title: "Bump the date library", engine: "claude", at: NOW - 17 * 86_400, needed: "nothing", checks: "passed" },
      { title: "Rename a test helper", engine: "claude", at: NOW - 18 * 86_400, needed: "nothing", checks: "passed" },
      { title: "Add a seed script", engine: "codex", at: NOW - 19 * 86_400, needed: "approval", checks: "passed" },
    ] satisfies HistoryMerge[],
    live: { needs_you: [], working: [], truncated: false },
    truncated: false,
  },
  "orbyn-engine": {
    project: "orbyn-engine", days: 60, as_of: NOW, min_rate_sample: 10, min_median_sample: 5,
    merges: 0, unaided: 0, reverted: 0, reverted_auto: 0, reverted_by_person: 0,
    cycle: null, weeks: [], engines: [], areas: [], reverts: [], recent: [],
    live: { needs_you: [], working: [], truncated: false },
    truncated: false,
  },
  "orbyn-docs": {
    project: "orbyn-docs", days: 60, as_of: NOW, min_rate_sample: 10, min_median_sample: 5,
    merges: 14, unaided: 12, reverted: 0, reverted_auto: 0, reverted_by_person: 0,
    cycle: { median_s: 18 * 60, p90_s: 40 * 60, fastest_s: 4 * 60, slowest_s: 70 * 60 },
    weeks: weeks([[2, 2, 0], [3, 3, 0], [2, 1, 0], [3, 3, 0], [2, 2, 0], [2, 1, 0]]),
    engines: [
      { name: "claude", model: "Sonnet 5.5", merges: 9, unaided: 8, reverted: 0, cost: tokens(900_000) },
      { name: "codex", model: "GPT-5.5", merges: 5, unaided: 4, reverted: 0, cost: tokens(380_000) },
    ],
    areas: [{ name: "guides", merges: 14, unaided: 12, reverted: 0 }],
    reverts: [], recent: [],
    live: { needs_you: [], working: [], truncated: false },
    truncated: false,
  },
};

const scale = (n: number, factor: number) => Math.round(n * factor);

/** The live row a scenario asks for: the default, one running worker cut short, or none at all as an older service sends. */
function withLiveScenario(answer: DashboardHistory, scenario: string): DashboardHistory {
  if (scenario === "crew-dashboard-live-none") {
    const older = { ...answer };
    delete older.live;
    return older;
  }
  if (scenario === "crew-dashboard-live-truncated") {
    return {
      ...answer,
      live: { needs_you: answer.live?.needs_you ?? [], working: [{ why: "running", count: 1 }], truncated: true },
    };
  }
  return answer;
}

/** The 60-day sample, or the same repository over 30 or 90 days: fewer or more weeks, counts scaled with them. The live row is never scaled. */
export function sampleHistory(project: string, days: Period, scenario = ""): DashboardHistory {
  const base = Object.hasOwn(BASE, project) ? BASE[project] : undefined;
  if (!base) throw new ApiFailure("not_found", "No such project.", 404);
  if (days === 60) return withLiveScenario(structuredClone(base), scenario);
  if (base.weeks.length === 0) return withLiveScenario({ ...structuredClone(base), days }, scenario);
  const factor = days / 60;
  const wanted = Math.max(1, Math.round(base.weeks.length * factor));
  const earlier = Array.from({ length: Math.max(0, wanted - base.weeks.length) }, (_, i) => ({ start: base.weeks[0]!.start - (i + 1) * WEEK, merges: 2, unaided: 1, reverted: 0 }));
  const kept = [...earlier.reverse(), ...base.weeks].slice(-wanted);
  const merges = kept.reduce((sum, week) => sum + week.merges, 0);
  const ratio = base.merges > 0 ? merges / base.merges : 0;
  const reverted = Math.min(merges, kept.reduce((sum, week) => sum + week.reverted, 0));
  return withLiveScenario({
    ...structuredClone(base),
    days,
    merges,
    unaided: Math.min(merges, scale(base.unaided, ratio)),
    reverted,
    reverted_auto: Math.min(reverted, base.reverted_auto),
    reverted_by_person: Math.min(reverted - Math.min(reverted, base.reverted_auto), base.reverted_by_person),
    weeks: kept,
    engines: base.engines.map((row) => ({ ...row, merges: scale(row.merges, ratio), unaided: scale(row.unaided, ratio), reverted: Math.min(scale(row.merges, ratio), row.reverted), cost: row.cost ? { ...row.cost, amount: scale(row.cost.amount, ratio) } : null })),
    areas: base.areas.map((row) => ({ ...row, merges: scale(row.merges, ratio), unaided: scale(row.unaided, ratio), reverted: Math.min(scale(row.merges, ratio), row.reverted) })),
    reverts: base.reverts.filter((row) => NOW - row.at <= days * 86_400),
  }, scenario);
}

/** The service's project ids, for the page's own route, against this sample's names. */
const SAMPLE_ID = new Map([["p-app", "orbyn-engineering"]]);

/** `GET /api/crew/dashboard` on the sample, for the mock API (tests and the live settings route). */
export const dashboardMock = {
  handle(scenario: string, method: string, path: string, query: URLSearchParams): MockResponse | null {
    if (method !== "GET" || path !== "/api/crew/dashboard") return null;
    const asked = query.get("project") ?? "";
    const requested = Number(query.get("days") ?? 60);
    const days: Period = requested === 30 || requested === 90 ? requested : 60;
    try {
      return { status: 200, body: sampleHistory(SAMPLE_ID.get(asked) ?? asked, days, scenario) };
    } catch (error) {
      const failure = error as { code?: string; message?: string; status?: number };
      return { status: failure.status ?? 404, body: { code: failure.code ?? "not_found", message: failure.message ?? "No such project." } };
    }
  },
};

const trustOf = (history: DashboardHistory): AutopilotTrust => {
  const row = (name: string, merges: number, reverts: number) => ({ name, rate: { merges, reverts, revert_permille: merges > 0 ? Math.round((reverts / merges) * 1000) : 0, counts: merges >= 10 } });
  return {
    window_days: history.days, min_sample: history.min_rate_sample, high_revert_permille: 150,
    total: row("", history.merges, history.reverted).rate,
    areas: history.areas.map((area) => row(area.name, area.merges, area.reverted)),
    engines: history.engines.map((engine) => row(engine.name, engine.merges, engine.reverted)),
    autonomy_by_area: true, truncated: false,
  };
};

const record = (merges: number, reverts: number) => ({ merges, reverts, escapes: null, window_days: 14 });
const area = (name: string, state: AutopilotAreaRow["state"], now: AutopilotAreaRow["now"], line: string, merges: number, reverts: number, since: number | null = null): AutopilotAreaRow => ({
  area: name, area_key: name, ceiling: "full", now, state, line, since, record: record(merges, reverts), recovery: null, override: null,
});
const AREAS: AutopilotAreas = {
  project: "orbyn-engineering", ceiling: "full", as_of: NOW, enforced: true, total: 4,
  rows: [
    area("web/auth", "demoted", "on", "2 reverts in 14 days", 9, 2, NOW - 14 * 86_400),
    area("docs/guides", "follows", "full", "Follows your ceiling", 12, 0),
    area("web/app", "follows", "full", "Follows your ceiling", 7, 0),
    area(".github/workflows", "not_enough_data", "full", "No change until there is data", 3, 0),
  ],
};
const THRESHOLDS: AutopilotThresholds = { window_days: 14, min_merges: 5, trip_reverts: 2, recovery_days: 7, recovery_merges: 5, proposed: true };

export interface CrewDashboardMock extends DashboardClient {
  /** Every read made, in order: `history:<project>:<days>`, `trust:<project>`, `areas:<project>`, `thresholds`, `area-history:<project>:<area>`. */
  reads: string[];
  /** Make the next history read fail. */
  failNext(): void;
}

export function createCrewDashboardMock(init: { delayMs?: number; failAll?: boolean; never?: boolean; withHistory?: boolean; noAreas?: boolean; scenario?: string } = {}): CrewDashboardMock {
  const reads: string[] = [];
  let failing = init.failAll === true;
  const gate = async (signal: AbortSignal) => {
    if (init.never) await new Promise<void>((_resolve, reject) => signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true }));
    await new Promise((resolve) => setTimeout(resolve, init.delayMs ?? 0));
    if (signal.aborted) throw new DOMException("Aborted", "AbortError");
    if (failing) {
      if (init.failAll !== true) failing = false;
      throw new ApiFailure("history_unreadable", "The run history on this device could not be read.", 500);
    }
  };
  return {
    reads,
    failNext() { failing = true; },
    async trust(project, signal) {
      reads.push(`trust:${project}`);
      await gate(signal);
      return trustOf(sampleHistory(project, 60));
    },
    async areas(project, signal) {
      reads.push(`areas:${project}`);
      await gate(signal);
      if (init.noAreas || project !== "orbyn-engineering") return { ...AREAS, project, total: 0, rows: [] };
      return structuredClone(AREAS);
    },
    async thresholds(signal) {
      reads.push("thresholds");
      await gate(signal);
      return { ...THRESHOLDS };
    },
    async areaHistory(project, areaKey, signal): Promise<AutopilotAreaHistory> {
      reads.push(`area-history:${project}:${areaKey}`);
      await gate(signal);
      const events = areaKey === "web/auth"
        ? [{ id: 1, kind: "demoted", from_mode: "full", to_mode: "on", decided_by: "breaker", cause: "abc1234", record: record(9, 2), at: NOW - 14 * 86_400 }]
        : [];
      return { project, area: areaKey, events, truncated: false };
    },
    history: init.withHistory === false ? null : async ({ project, days }, signal) => {
      reads.push(`history:${project}:${days}`);
      await gate(signal);
      return sampleHistory(project, days, init.scenario);
    },
  };
}
