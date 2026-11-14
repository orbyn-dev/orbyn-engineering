// What the Flow screens show from the Flow routes' answers: words, tones and counts. Pure functions, no copy beyond
// the words Crew Designs draws. Nothing here reads a check, a command or an excerpt: the routes carry none.
import type { FlowCell } from "../../../../generated/FlowCell";
import type { FlowCellState } from "../../../../generated/FlowCellState";
import type { FlowCounts } from "../../../../generated/FlowCounts";
import type { FlowFilter } from "../../../../generated/FlowFilter";
import type { FlowGate } from "../../../../generated/FlowGate";
import type { FlowLane } from "../../../../generated/FlowLane";
import type { FlowStage } from "../../../../generated/FlowStage";
import type { FlowTaskRow } from "../../../../generated/FlowTaskRow";

/** The ten stages in the order of the line, with the short name of a column and the full name of a stage. */
export const STAGES: { stage: FlowStage; column: string; name: string }[] = [
  { stage: "brief", column: "Brief", name: "Brief" },
  { stage: "route", column: "Route", name: "Route" },
  { stage: "build", column: "Build", name: "Build" },
  { stage: "checks", column: "Checks", name: "Checks" },
  { stage: "code", column: "Code", name: "Code review" },
  { stage: "security", column: "Security", name: "Security review" },
  { stage: "finding", column: "Finding", name: "Finding check" },
  { stage: "merge", column: "Merge", name: "Merge queue" },
  { stage: "main", column: "Main", name: "Main" },
  { stage: "you", column: "You", name: "You" },
];

export const FILTERS: { filter: FlowFilter; label: string }[] = [
  { filter: "all", label: "All" },
  { filter: "needs_you", label: "Needs you" },
  { filter: "reviewing", label: "Reviewing" },
  { filter: "failed", label: "Failed" },
  { filter: "passed", label: "Passed" },
];

/** A filter chip's count, from the counts the service gives over the whole crew (before any filter, search or engine). */
export function countOf(counts: FlowCounts, filter: FlowFilter): number {
  return filter === "all" ? counts.all : filter === "needs_you" ? counts.needs_you : counts[filter];
}

export type Tone = "ok" | "run" | "need" | "fail" | "pend" | "skip" | "rev";
export type Glyph = "ok" | "alert" | "eye" | "spin" | null;

const TONE: Record<FlowCellState, Tone> = { done: "ok", running: "run", needs_you: "need", failed: "fail", pending: "pend", skipped: "skip", review_waiting: "rev" };
const GLYPH: Record<FlowCellState, Glyph> = { done: "ok", running: "spin", needs_you: "alert", failed: "alert", pending: null, skipped: null, review_waiting: "eye" };
/** A word for a state, for a screen reader and the cell's tooltip; the cell itself draws a glyph and a count. */
const STATE_WORD: Record<FlowCellState, string> = { done: "Done", running: "Running", needs_you: "Needs you", failed: "Failed", pending: "Not reached", skipped: "Skipped", review_waiting: "Waiting for your review after the merge" };

export interface CellView {
  tone: Tone;
  glyph: Glyph;
  /** The count written in the cell: "3/3" for checks, "×3" for reviewer lanes, "–" for a skipped stage, or nothing. */
  text: string;
  /** What the cell says to a screen reader: the stage, its state and its count. */
  label: string;
}

/** One cell of a task's line. `passed` and `of` are checks; `lanes` is the reviewer lanes that ran. */
export function cellView(cell: FlowCell, name: string): CellView {
  const count = cell.passed != null && cell.of != null ? `${cell.passed}/${cell.of}` : cell.lanes != null && cell.lanes > 0 ? `×${cell.lanes}` : "";
  const text = cell.state === "skipped" ? "–" : cell.state === "pending" ? "" : count;
  return { tone: TONE[cell.state], glyph: GLYPH[cell.state], text, label: `${name}: ${STATE_WORD[cell.state]}${count ? ` ${count}` : ""}` };
}

/** The cell of a stage, or a not-reached one when the row carries fewer than ten. */
export function cellOf(row: FlowTaskRow, stage: FlowStage): FlowCell {
  return row.cells.find((cell) => cell.stage === stage) ?? { stage, state: "pending" };
}

export interface RoundView {
  tone: Tone | "plain";
  text: string;
}

/** The Round column: the loop count, red when the task failed or stopped, blue while it runs, "Queued" before it starts. */
export function roundView(row: FlowTaskRow): RoundView {
  if (row.state === "briefed") return { tone: "plain", text: "Queued" };
  const text = `Round ${row.round}`;
  if (row.state === "running" || row.state === "verifying" || row.state === "changed_approach" || row.state === "routed") return { tone: "run", text };
  if (row.state === "failed" || row.state === "stopped") return { tone: "fail", text };
  if (row.state === "needs_person") return { tone: "need", text };
  return { tone: "plain", text };
}

/** The word on a stage's chip in the zoomed stage list, in Crew Designs' own words. */
export function stageWord(stage: FlowStage, cell: FlowCell): { tone: "ok" | "info" | "warn" | "err" | "neutral"; text: string } {
  const { state } = cell;
  const tone = state === "done" ? "ok" : state === "running" ? "info" : state === "needs_you" ? "warn" : state === "failed" ? "err" : "neutral";
  const waiting = { tone, text: state === "skipped" ? "Skipped" : "Waiting" } as const;
  if (state === "pending" || state === "skipped") return waiting;
  if (state === "needs_you") return { tone, text: "Needs you" };
  if (state === "review_waiting") return { tone: "warn", text: "Waiting" };
  if (stage === "checks" && cell.passed != null && cell.of != null) return { tone, text: `${cell.passed} of ${cell.of}` };
  if (state === "failed") return { tone, text: "Failed" };
  if (state === "running") return { tone, text: stage === "code" || stage === "security" ? "Reviewing" : "Running" };
  switch (stage) {
    case "brief": return { tone, text: "Written" };
    case "route": return { tone, text: "Routed" };
    case "finding": return { tone, text: "Clear" };
    case "merge": return { tone, text: "Merged" };
    case "main": return { tone, text: "Healthy" };
    case "code":
    case "security": return { tone, text: "No findings" };
    default: return { tone, text: "Done" };
  }
}

/** What a lane says: its findings by check, else its state. */
export function laneWord(lane: FlowLane): string {
  if (lane.confirmed > 0) return `${lane.confirmed} confirmed`;
  if (lane.unconfirmed > 0) return `${lane.unconfirmed} unconfirmed`;
  if (lane.state === "running" || lane.state === "verifying" || lane.state === "routed" || lane.state === "briefed") return "Reviewing";
  if (lane.state === "passed") return "No findings";
  return lane.state.replaceAll("_", " ");
}

/** Who decided a gate, in the words of the design's legend. */
export function decidedBy(gate: FlowGate): string {
  return gate.decided_by === "code" ? "Code" : gate.decided_by === "confirmed_finding" ? "A confirmed finding" : gate.decided_by === "person" ? "You" : gate.decided_by.replaceAll("_", " ");
}

/** A service word such as `needs_person` or `looks_good` as plain text. */
export const plain = (word: string): string => word.replaceAll("_", " ");

/** A moment as the person's own clock reads it, for example "Sat 7 Nov, 15:02". An unreadable one is shown as sent. */
export function when(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString("en-GB", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

/** "8 of 31 shown": how many rows are drawn of those that match. */
export const shown = (rows: number, total: number): string => `${rows} of ${total} shown`;
