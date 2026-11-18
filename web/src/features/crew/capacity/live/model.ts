import type { EngineCoverage } from "../../../../generated/EngineCoverage";
import type { SamplePlan } from "../../../../generated/SamplePlan";

export const NOT_MEASURED = "Not measured";

export const count = (n: number) => n.toLocaleString("en-US");
const plural = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;
export const merges = (n: number) => plural(n, "merge", "merges");
export const changes = (n: number) => plural(n, "change", "changes");

/** A unix-seconds moment as a date, in the viewer's zone: "Mon 12 Oct 2026". */
export function dateOf(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short", year: "numeric" });
}
export function momentOf(unix: number): string {
  const at = new Date(unix * 1000);
  return `${dateOf(unix)}, ${at.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}`;
}

/** The three splits of an area's lines. "Not in a queue" is what is neither reviewed nor waiting; it never goes below zero. */
export function split(row: { agent_lines: number; reviewed_lines: number; in_queue_lines: number }) {
  const unread = Math.max(0, row.agent_lines - row.reviewed_lines);
  return { unread, notInQueue: Math.max(0, unread - row.in_queue_lines) };
}

/** A share of a total, whole percent; absent when there is no total (nothing to divide by is Not measured, never 0). */
export const share = (part: number, whole: number): string | null => (whole > 0 ? `${Math.round((part / whole) * 100)}%` : null);

export const engineLabel = (row: EngineCoverage) => (row.model ? `${row.engine} · ${row.model}` : `${row.engine} · model ${NOT_MEASURED.toLowerCase()}`);

/** What a row of the sampling plan leaves unsampled: "0 to 3". */
export const notSampled = (row: SamplePlan) => `${row.from - row.sampled} to ${row.to - row.sampled}`;

