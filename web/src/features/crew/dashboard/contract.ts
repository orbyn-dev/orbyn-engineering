/**
 * The Crew dashboard's reads (parity W12). Two come from routes that exist and one does not:
 *
 * - `trust` is `GET /api/crew/trust?project=`: merges and reverts of one repository over the service's window, by engine and by area.
 * - `areas`, `thresholds` and `areaHistory` are the Autopilot by area reads the Settings page already uses.
 * - `history` is `GET /api/crew/dashboard?project=&days=`, written in docs/crew-dashboard.md (Eng 2) and not served yet. The production
 *   client has none (`history: null`): it sends nothing, and the page draws Planned where only that read could answer. The mock in
 *   web/mock/crewDashboard.ts exists for tests and the dev-only Preview frames, which hand it to the page directly.
 *
 * Every string here is text from the disk, a pull request or an engine: untrusted. The screens draw it as text, capped, never as markup or a link.
 */
import { request } from "../../../api/client";
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
  /** The planned history read. `null`: the service does not serve it yet, so nothing is sent. */
  history: ((query: { project: string; days: Period }, signal: AbortSignal) => Promise<DashboardHistory>) | null;
}

/** The production client: the existing reads, and no history read at all. */
const production: DashboardClient = {
  trust: (project, signal) => request<AutopilotTrust>("GET", `/api/crew/trust?${new URLSearchParams({ project })}`, undefined, signal),
  areas: (project, signal) => crewApi.areas(project, signal),
  thresholds: (signal) => crewApi.thresholds(signal),
  areaHistory: (project, area, signal) => crewApi.areaHistory(project, area, signal),
  history: null,
};

let current: DashboardClient = production;

export const dashboardClient = (): DashboardClient => current;

/** Tests install a client here; `null` puts back the production default, which sends no history read. */
export function setDashboardClient(next: DashboardClient | null): void {
  current = next ?? production;
}
