// The Needs you inbox's row. Every source (held commands, stops, the Review queue, background runs) maps to one
// `NeedsItem`; the screens draw nothing else. A field a source does not state is null, never a guess.
import type { Risk } from "../autopilot/approvals/model";

/** The kinds the designs name, in the order their filters are drawn. The first four live kinds have a route behind them. */
export const NEEDS_KINDS = ["autonomy", "approval", "denied", "questions", "review", "main", "conflict", "stopped", "background"] as const;
export type NeedsKind = (typeof NEEDS_KINDS)[number];

/** The kinds `assemble.ts` builds from a route today. The rest are drawn by the frames only. */
export const LIVE_KINDS = ["approval", "stopped", "main", "background"] as const satisfies readonly NeedsKind[];

/** A filter chip's word, from the designs: Approvals, Denied, Questions, Main, Conflicts, Reviews, Autonomy. */
export const KIND_FILTER: Record<NeedsKind, string> = {
  autonomy: "Autonomy",
  approval: "Approvals",
  denied: "Denied",
  questions: "Questions",
  review: "Reviews",
  main: "Main",
  conflict: "Conflicts",
  stopped: "Stops",
  background: "Background",
};

/** The word before a narrow card's risk: "Approval · High · hard to undo". */
export const KIND_ONE: Record<NeedsKind, string> = {
  autonomy: "Autonomy",
  approval: "Approval",
  denied: "Denied",
  questions: "Questions",
  review: "Review",
  main: "Main",
  conflict: "Conflict",
  stopped: "Stopped",
  background: "Background",
};

/** An answer an item offers. `key` names what the screen's owner does; none is a merge, a revert or an approval of its own. */
export interface NeedsAction {
  key: "approve" | "deny" | "dismiss" | "open" | "decide" | "open-both" | "answer" | "rerun" | "review" | "override" | "later" | "open-review" | "open-thread";
  label: string;
  primary?: boolean;
}

export interface NeedsItem {
  /** Stable across reads: `<source>:<id>`. */
  id: string;
  kind: NeedsKind;
  title: string;
  /** The risk when the source states it. Only a review item draws its chips (v37); the break on main keeps its risk for the ranking alone. */
  risk: Risk | null;
  /** Reversibility is stated by no route: only the frames carry it. */
  undo: "easy" | "hard" | null;
  /** Who asked and what it is, after the tags: "Claude Code · Sonnet 5.5 · approval". */
  from: string;
  /** What waiting costs, in the designs' sentence for the kind; null when no source says. */
  cost: string | null;
  /** Milliseconds since the epoch when it began waiting; null when the source does not say. */
  since: number | null;
  /** Past its review time. With High risk, the only reason a risk chip is amber. */
  overdue?: boolean;
  /** A short line under the cost: what an action does, when the screen must say so ("Dismiss hides this stop on this device."). */
  hint?: string;
  actions: NeedsAction[];
  /** The crew's name, for the narrow card's second line. */
  engine?: string;
  /** The source's own id the screen's handlers act on (a held command's id, a task id, a crew id). */
  ref: string;
}

/** The inbox as a filter and a list. */
export interface NeedsView {
  items: NeedsItem[];
  /** Filter chips in the designs' order, with the count of each present kind. */
  filters: { kind: NeedsKind; label: string; count: number }[];
}
