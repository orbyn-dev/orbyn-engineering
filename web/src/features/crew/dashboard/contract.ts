/**
 * The Crew dashboard's reads (parity W12). Two come from routes that exist and one does not:
 *
 * - `trust` is `GET /api/crew/trust?project=`: merges and reverts of one repository over the service's window, by engine and by area.
 * - `areas`, `thresholds` and `areaHistory` are the Autopilot by area reads the Settings page already uses.
 * - `history` is `GET /api/crew/dashboard?project=&days=&utc_offset_minutes=`, written in docs/crew-dashboard.md (Eng 2). The production
 *   client sends it; a service without the route answers `404` and the page falls back to the trust record (`readModel`), drawing Planned
 *   where only the history could answer. The mock in web/mock/crewDashboard.ts is for tests and the dev-only Preview frames.
 *
 * Every string here is text from the disk, a pull request or an engine: untrusted. The screens draw it as text, capped, never as markup or a link.
 */
import { request } from "../../../api/client";
import type { CrewDashboard } from "../../../generated/CrewDashboard";
import type { AutopilotAreaHistory } from "../../../generated/AutopilotAreaHistory";
import type { AutopilotAreas } from "../../../generated/AutopilotAreas";
import type { AutopilotThresholds } from "../../../generated/AutopilotThresholds";
import type { AutopilotTrust } from "../../../generated/AutopilotTrust";
import { crewApi } from "../autopilot/approvals/api";

/** The periods the page offers. */
export const PERIODS = [30, 60, 90] as const;
export type Period = (typeof PERIODS)[number];

export interface HistoryWeek {
  /** Unix seconds of the week's first day. */
  start: number;
  merges: number;
  /** Merges that needed nothing from the person after the brief. */
  unaided: number;
  reverted: number;
}

export interface HistoryEngine {
  name: string;
  model: string | null;
  merges: number;
  unaided: number;
  reverted: number;
  /** In the engine's own unit; null when the engine reports no cost. Never added across engines. */
  cost: { unit: string; amount: number } | null;
}

export interface HistoryArea {
  name: string;
  merges: number;
  unaided: number;
  reverted: number;
}

export interface HistoryRevert {
  title: string;
  /** Unix seconds. */
  at: number;
  by: "person" | "auto";
  /** Whether the cause is certain; null when the service does not say. */
  certain: boolean | null;
}

export interface HistoryMerge {
  title: string;
  engine: string;
  at: number;
  /** What the merge needed from the person after the brief. */
  needed: "nothing" | "answer" | "approval" | "other";
  checks: "passed" | "failed" | "none";
}

export interface DashboardHistory {
  project: string;
  days: number;
  as_of: number;
  /** A rate shows from this many merges (10). */
  min_rate_sample: number;
  /** A median shows from this many merges (5). */
  min_median_sample: number;
  merges: number;
  unaided: number;
  reverted: number;
  reverted_auto: number;
  reverted_by_person: number;
  /** Seconds from the brief to the merge; null while no merge has both times. */
  cycle: { median_s: number | null; p90_s: number | null; fastest_s: number | null; slowest_s: number | null } | null;
  weeks: HistoryWeek[];
  engines: HistoryEngine[];
  areas: HistoryArea[];
  reverts: HistoryRevert[];
  recent: HistoryMerge[];
  truncated: boolean;
}

export interface DashboardClient {
  /** One repository's trust record. */
  trust(project: string, signal: AbortSignal): Promise<AutopilotTrust>;
  areas(project: string, signal: AbortSignal): Promise<AutopilotAreas>;
  thresholds(signal: AbortSignal): Promise<AutopilotThresholds>;
  areaHistory(project: string, area: string, signal: AbortSignal): Promise<AutopilotAreaHistory>;
  /** The history read. `null` in a test client that has none: only the trust record is read then. */
  history: ((query: { project: string; days: Period }, signal: AbortSignal) => Promise<DashboardHistory>) | null;
}

/** The service's answer with its omitted fields made explicit: the page reads `null` for "not measured", never a missing key. */
export function fromService(answer: CrewDashboard): DashboardHistory {
  const cycle = answer.cycle;
  return {
    ...answer,
    cycle: cycle ? { median_s: cycle.median_s ?? null, p90_s: cycle.p90_s ?? null, fastest_s: cycle.fastest_s ?? null, slowest_s: cycle.slowest_s ?? null } : null,
    engines: answer.engines.map((engine) => ({ ...engine, model: engine.model ?? null, cost: engine.cost ?? null })),
    reverts: answer.reverts.map((revert) => ({ ...revert, certain: revert.certain ?? null })),
  };
}

/** The person's own offset from UTC in minutes, as the service takes it: it decides which Monday a week starts on. */
const utcOffsetMinutes = (): number => {
  const offset = -new Date().getTimezoneOffset();
  return Number.isFinite(offset) && offset >= -840 && offset <= 840 ? offset : 0;
};

/** The production client: the existing reads and the dashboard history read. */
const production: DashboardClient = {
  trust: (project, signal) => request<AutopilotTrust>("GET", `/api/crew/trust?${new URLSearchParams({ project })}`, undefined, signal),
  areas: (project, signal) => crewApi.areas(project, signal),
  thresholds: (signal) => crewApi.thresholds(signal),
  areaHistory: (project, area, signal) => crewApi.areaHistory(project, area, signal),
  history: async (query, signal) =>
    fromService(
      await request<CrewDashboard>(
        "GET",
        `/api/crew/dashboard?${new URLSearchParams({ project: query.project, days: String(query.days), utc_offset_minutes: String(utcOffsetMinutes()) })}`,
        undefined,
        signal,
      ),
    ),
};

let current: DashboardClient = production;

export const dashboardClient = (): DashboardClient => current;

/** Tests install a client here; `null` puts back the production default. */
export function setDashboardClient(next: DashboardClient | null): void {
  current = next ?? production;
}
