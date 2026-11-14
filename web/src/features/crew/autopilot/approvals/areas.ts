// Autopilot by area (Crew Designs v39, frames `autonomy-*`): what each row and panel says, worked out from the service's reads.
// The service sends the record and the state; the words are the design's, so they are written here from those fields and
// never from the service's own sentence, which is shown only where the design draws no row for the state.
import type { AutopilotAreaEvent } from "../../../../generated/AutopilotAreaEvent";
import type { AutopilotAreaRow } from "../../../../generated/AutopilotAreaRow";
import type { AutopilotMode } from "../../../../generated/AutopilotMode";
import type { AutopilotThresholds } from "../../../../generated/AutopilotThresholds";
import { MODE_COPY } from "./copy";

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const two = (n: number) => String(n).padStart(2, "0");

/** "Sat 7 Nov, 15:36", in the person's own time zone. */
export function stamp(seconds: number): string {
  const at = new Date(seconds * 1000);
  return `${DAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]}, ${two(at.getHours())}:${two(at.getMinutes())}`;
}

/** "Sat 7 Nov 2026, 15:36": the line under the chips. */
export function asOf(seconds: number): string {
  const at = new Date(seconds * 1000);
  return `${DAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]} ${at.getFullYear()}, ${two(at.getHours())}:${two(at.getMinutes())}`;
}

const clockOf = (seconds: number) => {
  const at = new Date(seconds * 1000);
  return `${two(at.getHours())}:${two(at.getMinutes())}`;
};
export { clockOf as clock };

export const modeName = (mode: AutopilotMode) => MODE_COPY[mode].name;
export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The chip around a mode is quiet when the mode is Off or when the area sits below its ceiling. */
export const quiet = (mode: AutopilotMode, ceiling: AutopilotMode, now: boolean) => mode === "off" || (now && mode !== ceiling);

/** Where a lowered area stands, for the rows that carry a panel of their own. */
export const isLowered = (row: AutopilotAreaRow) => row.state === "demoted" || row.state === "recovering" || row.state === "criteria_met";

export type RowView = {
  /** The line under the mode in the Now column. */
  why: string;
  /** "2 of 9" or "Not measured" for the Reverted column, and whether it is a measurement. */
  reverted: string;
  measured: boolean;
  /** The Window column: the days and the sample under them. */
  days: string;
  merges: string;
};

export function rowView(row: AutopilotAreaRow, restored: AutopilotAreaEvent | null): RowView {
  const record = row.record;
  const days = `${record.window_days} days`;
  const sample = plural(record.merges, "merge");
  let why: string;
  switch (row.state) {
    case "follows":
      // A window that already trips but is not recorded yet has no row in the design: the service's own sentence says so.
      why = record.reverts > 0 && row.line.includes("evaluated") ? row.line : "Follows your ceiling";
      break;
    case "not_enough_data":
      why = "No change until there is data";
      break;
    case "ceiling_lower":
      why = `Your ceiling is ${modeName(row.ceiling)}: the record is shown, nothing changes`;
      break;
    case "demoted":
    case "recovering":
    case "criteria_met":
      why = row.ceiling !== "full"
        ? `Your ceiling is ${modeName(row.ceiling)}: it wins over the demotion`
        : row.state === "demoted"
          ? `Demoted ${row.since ? `${stamp(row.since)}: ` : ""}${plural(record.reverts, "revert")} in ${record.window_days} days`
          : row.state === "criteria_met"
            ? "Criteria met; returns at the next merge"
            : `Recovering: day ${row.recovery?.clean_days ?? 0} of ${row.recovery?.days_needed ?? 0} with no revert; ${row.recovery?.merges ?? 0} of ${row.recovery?.merges_needed ?? 0} merges since, none reverted`;
      break;
    case "overridden":
      why = row.line;
      break;
  }
  if (restored && row.state === "follows") why = `Restored ${stamp(restored.at)} at a merge`;
  if (row.state === "not_enough_data") return { why, reverted: "Not measured", measured: false, days, merges: sample };
  // A lowered area counts its way back: the days since the last revert and the merges since, none reverted.
  if ((row.state === "recovering" || row.state === "criteria_met") && row.recovery) {
    return { why, reverted: `0 of ${row.recovery.merges}`, measured: true, days: `${row.recovery.clean_days} days`, merges: plural(row.recovery.merges, "merge") };
  }
  return { why, reverted: `${record.reverts} of ${record.merges}`, measured: true, days, merges: sample };
}

/** The newest event of an area's trail, when it is a return still inside the window the table shows. */
export function recentRestore(events: AutopilotAreaEvent[] | undefined, now: number, windowDays: number): AutopilotAreaEvent | null {
  const newest = events?.[0];
  return newest && newest.kind === "restored" && now - newest.at <= windowDays * 86_400 ? newest : null;
}

export const trips = (t: AutopilotThresholds) => `${plural(t.min_merges, "merge")} and ${t.trip_reverts} reverted`;

/**
 * The frame of Crew Designs v39 the table's data is in, for `data-frame`: the first state that has a panel of its own, else the table.
 * The override menu and the warning are drawn over the table, so the page names them while they are open.
 */
export function frameOf(rows: AutopilotAreaRow[], restored: boolean): string {
  if (rows.some((row) => isLowered(row) && row.ceiling === "off")) return "autonomy-demoted-ceiling-lowered";
  if (rows.some((row) => row.state === "demoted" && row.ceiling === "full")) return "autonomy-tripped";
  if (rows.some((row) => row.state === "recovering" && row.ceiling === "full")) return "autonomy-recovering";
  if (rows.some((row) => row.state === "criteria_met" && row.ceiling === "full")) return "autonomy-criteria-met";
  if (restored) return "autonomy-restored";
  if (rows.length > 0 && rows.every((row) => row.state === "not_enough_data")) return "autonomy-lowdata";
  return "autonomy-table";
}
