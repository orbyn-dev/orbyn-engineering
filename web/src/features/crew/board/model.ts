// What the Taskboard shows from the board route's answer. Pure functions; the words are Crew Designs' own.
import type { FlowCard } from "../../../generated/FlowCard";
import type { FlowColumnId } from "../../../generated/FlowColumnId";

export interface ColumnSpec {
  id: FlowColumnId;
  label: string;
  /** The chip tone of the header. Needs you is the only amber. */
  tone: "neutral" | "info" | "warn" | "ok";
  /** A spinner beside the name: the work in this column is moving. */
  working: boolean;
  /** Drawn as a narrow rail until it is opened. */
  rail: boolean;
}

/** The seven columns, in the order work moves through them. */
export const COLUMNS: ColumnSpec[] = [
  { id: "queued", label: "Queued", tone: "neutral", working: false, rail: false },
  { id: "routed", label: "Routed", tone: "neutral", working: false, rail: false },
  { id: "running", label: "Running", tone: "info", working: true, rail: false },
  { id: "verifying", label: "Verifying", tone: "info", working: true, rail: false },
  { id: "needs_you", label: "Needs you", tone: "warn", working: false, rail: false },
  { id: "passed", label: "Passed", tone: "ok", working: false, rail: true },
  { id: "stopped_cancelled", label: "Stopped · Cancelled", tone: "neutral", working: false, rail: true },
];

/** "now", "1m ago", "3h ago", "2d ago": the time a task last changed, as the card says it after "updated". An unreadable time is shown as sent. */
export function ago(iso: string, now: number = Date.now()): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return iso;
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 60) return "now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** The effort a card may show: Low or Medium, as the beta routes no other. Anything else is left out, never guessed. */
export function effortLabel(effort: string | null | undefined): string | null {
  return effort === "low" ? "Low" : effort === "medium" ? "Medium" : null;
}

/** The second line of a card's engine block: the model, then the effort. Empty until the task is routed. */
export function modelLine(card: Pick<FlowCard, "model" | "effort">): string {
  return [card.model, effortLabel(card.effort)].filter(Boolean).join(" · ");
}

/** "3 of 4 checks passed": the counts a card carries once its task has reached verifying. Nothing is shown for an absent or impossible pair. */
export function checksLine(card: Pick<FlowCard, "checks">): string | null {
  const counts = card.checks;
  if (!counts) return null;
  const { passed, of } = counts;
  if (!Number.isInteger(passed) || !Number.isInteger(of) || passed < 0 || of < 1 || passed > of) return null;
  return `${passed} of ${of} ${of === 1 ? "check" : "checks"} passed`;
}

/** "Working · checks running": why a card's worker owns live work now. A value a later service sends is left out, never guessed. */
export function workingLine(working: string | null | undefined): string | null {
  switch (working) {
    case "running":
      return "Working";
    case "verifying":
      return "Working · checks running";
    case "retry":
      return "Working · retrying after a failed check";
    case "fix":
      return "Working · fixing a review finding";
    default:
      return null;
  }
}

export interface WaitingLine {
  /** The sentence's bold lead, on the kinds that name the wait itself. */
  lead?: string;
  /** What follows the lead after " · ", or the whole line when there is no lead. */
  text?: string;
}

/** The amber sentence a waiting card draws: the kind's own words where it has them, else the card's reason as before. */
export function waitingLine(card: { needs_you?: string | null; reason?: string | null }): WaitingLine | null {
  const reason = card.reason?.trim() || null;
  switch (card.needs_you) {
    case "task_needs_person":
      return reason ? { lead: "Asked you a question", text: reason } : { lead: "Asked you a question" };
    case "merge_waiting":
      return { text: "The merge waits for you" };
    case "manual_check":
    default:
      return reason ? { text: reason } : null;
  }
}

/** Cards of a column still to read, from the count the service gives and the cards held. */
export const moreOf = (count: number, held: number): number => Math.max(0, count - held);
