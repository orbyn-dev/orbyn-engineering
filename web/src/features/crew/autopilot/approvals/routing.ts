// A `route_class` ledger row as the Approval log's Routing view draws it. The row is the one the routing wrote: the classes the brief was
// in (with up to 20 sample paths each), each engine a rule blocked, the engine chosen, and the commit and blob of `.orbyn/crew.json`.
// The stored row has the task's id but not its title, so the id stands for it.
import type { AutopilotLogRow } from "../../../../generated/AutopilotLogRow";
import type { CrewRouteBlocked } from "../../../../generated/CrewRouteBlocked";
import type { CrewRouteClassHit } from "../../../../generated/CrewRouteClassHit";

/** Rows of the ledger the Routing view lists. */
export const ROUTING_KIND = "route_class";

/** Sample paths shown on a row; the rest are "N more". */
export const SAMPLES_SHOWN = 3;

export interface RoutingRow {
  id: string;
  time: string;
  /** The ledger row's own time, for the day line. */
  at: string;
  task: string;
  classes: CrewRouteClassHit[];
  blocked: CrewRouteBlocked[];
  chosen: string | null;
  commit: string | null;
  blob: string | null;
  /** Why the file was invalid, when it was: every engine was blocked. */
  problem: string | null;
  result: "routed" | "none";
}

export function clockOf(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
}

const list = <T,>(value: unknown): T[] => (Array.isArray(value) ? (value as T[]) : []);
const text = (value: unknown) => (typeof value === "string" && value !== "" ? value : null);

export function routingRow(row: AutopilotLogRow): RoutingRow | null {
  if (row.kind !== ROUTING_KIND) return null;
  const body = (row.body ?? {}) as Record<string, unknown>;
  const chosen = text(body.chosen);
  return {
    id: String(row.id),
    time: clockOf(row.at),
    at: row.at,
    task: row.task_id,
    classes: list<CrewRouteClassHit>(body.classes),
    blocked: list<CrewRouteBlocked>(body.blocked),
    chosen,
    commit: text(body.commit),
    blob: text(body.blob),
    problem: text(body.problem),
    result: chosen ? "routed" : "none",
  };
}

/** The sample paths of a row, each once, and how many more there are than are shown. */
export function samplesOf(row: RoutingRow): { shown: string[]; more: number } {
  const all = [...new Set(row.classes.flatMap((one) => one.samples))];
  return { shown: all.slice(0, SAMPLES_SHOWN), more: Math.max(0, all.length - SAMPLES_SHOWN) };
}

/** A hash as the log shows it: seven characters. */
export const short = (hash: string | null) => (hash ? hash.slice(0, 7) : null);
