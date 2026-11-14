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

/** Cards of a column still to read, from the count the service gives and the cards held. */
export const moreOf = (count: number, held: number): number => Math.max(0, count - held);
