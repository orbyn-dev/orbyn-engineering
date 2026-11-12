/**
 * View models for the Autopilot review screens (risk, Review queue, Merge queue, watch on main, digest).
 * They are display-ready and hand-written: the Rust side (crew/autopilot/merge) has no HTTP route and no
 * generated binding yet, so nothing here is a wire type. A mapper from the future bindings fills these.
 */

export type Tier = "low" | "medium" | "high";
export const TIER_LABEL: Record<Tier, string> = { low: "Low", medium: "Medium", high: "High" };

/** How one reason moved the tier. */
export type Effect = "raises" | "lowers" | "none";
export const EFFECT_LABEL: Record<Effect, string> = { raises: "Raises risk", lowers: "Lowers risk", none: "No effect" };

export type ReasonIcon = "paths" | "size" | "coverage" | "revert-rate" | "area-mode" | "security";

export interface Reason {
  icon: ReasonIcon;
  label: string;
  value: string;
  effect: Effect;
}

/** A task's tier with every reason behind it. A tier is never shown without these. */
export interface RiskReport {
  tier: Tier;
  title: string;
  reasons: Reason[];
  /** The box under the reasons: what the tier means for the merge and the review. */
  outcome?: { heading: string; text: string };
  /** The tier was raised from another one, as when the Security review ran on the author's engine. */
  raisedFrom?: Tier;
}

export type EntryState = "waiting" | "overdue" | "reverted";

/** One merge to main in the Review queue, or a batch of Low merges. */
export interface QueueEntry {
  id: string;
  tier: Tier;
  title: string;
  /** Who merged it, when, with what, and where. Display text. */
  meta: string;
  /** Neutral marker when the merge came from a demoted or promoted area. */
  marker?: string;
  /** The proof in a few short phrases. */
  proof: string[];
  state: EntryState;
  /** Highest-ranked item: it gets the stronger outline. */
  top?: boolean;
  /** The line under the card: the review window, or why it is overdue or reverted. */
  footer: string;
  /** Present on an overdue item: the tag before the footer. */
  overdueTag?: string;
  /** More than one Low merge reviewed together. */
  batchSize?: number;
}

export interface QueueBanner {
  tone: "amber" | "neutral";
  heading: string;
  text: string;
  actions: string[];
}

export type QueueFilter = "to-review" | "high" | "overdue" | "reverted";

export interface QueueData {
  entries: QueueEntry[];
  banner?: QueueBanner;
  /** Chip counts when the drawn list is a clipped sample; otherwise they are counted from the entries. */
  counts?: Partial<Record<QueueFilter, number>>;
  health: string;
}

export interface Claim {
  text: string;
  evidence: string;
  ok: boolean;
}

/** One merge opened: the sentence, the diff, the claims with their checks and what was not verified. */
export interface MergeItem {
  title: string;
  tier: Tier;
  merged: string;
  summary: string;
  builtBy: string;
  mergedAt: string;
  /** Time spent, with no money figure: per-merge cost is not settled across engines. */
  spent: string;
  reviewBy: string;
  diff: { kind: "add" | "del"; text: string }[];
  claims: Claim[];
  notVerified: string[];
  reviewedBy: { lane: string; who: string }[];
  /** Later merges that depend on this one: a revert asks before it touches them. */
  dependents: { commit: string; title: string; note: string }[];
  /** `reason` is a draft the person may already have typed; empty from the backend. */
  revert: { commit: string; later: string; spent: string; together: string; reason?: string };
  /** `prompt` is the text already typed; `draft` is the brief Crew shows before it sends. */
  commentBrief: { prompt: string; draft: string };
}

export interface BatchRow {
  title: string;
  detail: string;
  /** What the change cost, in the units of the engine that did it. */
  cost: string;
  checked: boolean;
}

export interface BatchData {
  heading: string;
  lead: { heading: string; text: string };
  group: string;
  groupSummary: string;
  rows: BatchRow[];
  total: number;
}

export type RowState = "merged" | "running" | "waiting" | "removed";

export interface MergeQueueRow {
  n: number;
  title: string;
  detail: string;
  state: RowState;
  status: string;
  tier: Tier;
}

export interface MergeQueueData {
  heading: string;
  summary: string;
  rows: MergeQueueRow[];
  notes: { heading: string; items?: string[]; text?: string }[];
}

export interface ForecastData {
  headline: string;
  conflict: string;
  facts: { label: string; value: string }[];
  order: { n: number; title: string; detail: string; status: string; running: boolean }[];
  why: { heading: string; text: string };
}

export type CommitState = "untested" | "passes" | "testing" | "fails" | "not-needed";

export interface CommitRow {
  sha: string;
  title: string;
  merged: string;
  state: CommitState;
}

export interface WatchEvidence {
  text: string;
  evidence: string;
  note?: string;
  ok: boolean;
}

export interface WatchData {
  health: string;
  strip: { tone: "bad" | "ok"; heading?: string; text: string; actions: { label: string; primary?: boolean }[] };
  ask?: { heading: string; text: string; actions: { label: string; primary?: boolean }[] };
  list?: { heading: string; summary: string; rows: CommitRow[] };
  evidence?: { heading: string; summary: string; rows: WatchEvidence[] };
  after?: { heading: string; text: string };
}

export interface DigestGroup {
  count: number;
  heading: string;
  summary: string;
  rows: { tier: Tier; title: string; at: string }[];
}

export interface DigestData {
  date: string;
  groups: DigestGroup[];
  also?: { heading: string; text: string };
  quiet?: { heading: string; text: string; action: string };
}

/** The board's tier columns (Taskboard). */
export interface BoardColumn {
  label: string;
  tone: "ok" | "warn";
  cards: { title: string; engine: string; model: string; checks: string; paths: string; tier: Tier; line: string; needs?: boolean }[];
}
