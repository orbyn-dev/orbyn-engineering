import { ApiFailure, NETWORK_ERROR } from "../../../../api/client";
import type { EscapeGap } from "../../../../generated/EscapeGap";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import type { EscapeSource } from "../../../../generated/EscapeSource";

/** Why each Planned pill on these screens is not built, in the words its pill gives. The design source names each one. */
export const WHY = {
  report: "GET /api/crew/escapes/report (per week, engine, area and lane, rates, gap and check counts) is not built",
  forge: "v1 reads this device’s own repository checks only; a forge’s CI status is not read",
  undo: "there is no undo in v1; confirmed and dismissed are final",
  model: "the merge does not record a model or a lane yet, so a row names the engine and the area only",
  counts: "GET /api/crew/escapes/counts (per engine for a task’s area) is not built",
  queue: "the Review queue is not in the beta, so there is no button to open it",
  check: "the create-check screen (the form for the path, what it asserts and the run command) is still a drawn sample, so no live screen collects a proposal yet",
  tooling: "a created check only adds test files in v1; a tooling gap is logged with no proposal",
  pruning: "pruning the oldest dismissed and possible escapes is not built; the writer refuses instead",
} as const;

/** The core’s own cap on one project’s escapes (ESCAPES_PER_PROJECT_MAX). The log read carries the count, so the page can
 * say the limit is reached without inventing a state the read does not carry. */
export const ESCAPES_LIMIT = 5000;

/** A refused write in the service’s own words: a stale act adds “Look again.”, a network failure its own message. */
export const refusalOf = (error: unknown): string => {
  const failure = error instanceof ApiFailure ? error : new ApiFailure("unexpected", "Something went wrong.", 0);
  return failure.code === NETWORK_ERROR ? failure.message : failure.code === "stale" ? `${failure.message} Look again.` : `${failure.code}: ${failure.message}`;
};

export const count = (n: number) => n.toLocaleString("en-US");
const plural = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;
export const merges = (n: number) => plural(n, "merge", "merges");

/** A unix-seconds moment as "Sat 7 Nov 2026, 17:00" (the log's As-of line). */
export function asOf(unix: number): string {
  const at = new Date(unix * 1000);
  return `${at.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short", year: "numeric" })}, ${timeOf(at)}`;
}

const timeOf = (at: Date) => at.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

/** A unix-seconds moment as "Sat 7 Nov, 15:34" (a table row or a dated timeline row; no year, as the design draws). */
export function dayMoment(unix: number): string {
  const at = new Date(unix * 1000);
  return `${at.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" })}, ${timeOf(at)}`;
}

/** A timeline row's time: the time alone on the day the escape was found, a date and time on any other day. */
export function eventMoment(unix: number, foundUnix: number): string {
  const day = (at: Date) => at.toLocaleDateString("en-GB");
  const at = new Date(unix * 1000);
  return day(at) === day(new Date(foundUnix * 1000)) ? timeOf(at) : dayMoment(unix);
}

/** Where an escape was found, in the design's words. The read does not say who reverted, so a revert is just "Reverted". */
export const sourceLabel = (source: EscapeSource): string =>
  source === "revert"
    ? "Reverted"
    : source === "person_marked"
      ? "Marked by you"
      : source === "post_merge_check"
        ? "Check failed after the merge"
        : "Later fix on the same lines";

/** The source's own title in a timeline row: "a revert", "you marked it", and so on. */
export const sourceTitle = (source: EscapeSource): string =>
  source === "revert"
    ? "a revert"
    : source === "person_marked"
      ? "you marked it"
      : source === "post_merge_check"
        ? "a check failed after the merge"
        : "a later fix";

export const stateLabel: Record<EscapeRow["state"], string> = {
  possible: "Possible escape",
  confirmed: "Confirmed escape",
  dismissed: "Dismissed",
};

export const gapLabel: Record<EscapeGap, string> = { test_gap: "Test gap", tooling_gap: "Tooling gap" };

/** The check's own one-plain-sentence line already says where it stands; the chip repeats it at a glance. */
export const checkStateLabel = (state: string): string => `Check ${state}`;

/**
 * The escape's own name: the reason the person or the scan gave when one was recorded (a mark's note is its reason), else
 * its one-sentence line. The merge's task title is not in any read, so nothing here invents one.
 */
export const nameOf = (row: EscapeRow): string => row.reason ?? row.line;
