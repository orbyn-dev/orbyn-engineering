// What the Autopilot screens draw. These shapes are the contract the Crew routes must answer
// (docs/crew.md, Autopilot): the core has the ledger rows and the modes, but no route serves them yet,
// so the mock API serves these and the screens fail closed when the service does not answer.

export type Mode = "off" | "on" | "full";
export const MODES: readonly Mode[] = ["off", "on", "full"];

export type Risk = "low" | "medium" | "high";

/** One engine as a person reads it: the program, the model and the vendor that signs it in. */
export interface EngineRef {
  engine: string;
  model: string;
  vendor: string;
}

export type ResultKind =
  | "allowed"
  | "denied_rule"
  | "denied_lead"
  | "keys_disagree"
  | "person_only"
  | "no_second_key"
  | "held_you"
  | "held_second_key"
  | "approved_you"
  | "denied_second_key"
  | "approved_two_keys";

export type HardKind = "always_allowed" | "passed" | "denied" | "person_only" | "guard" | "guard_two_keys";

export type LeadAnswer =
  | { state: "not_asked" }
  /** `short` is the table cell's words, `reason` the whole sentence the drawer shows. */
  | { state: "yes" | "no"; reason: string; short: string };

export type SecondAnswer =
  /** `yours`: a guarded command held for the person; `you`: the person approved it once. */
  | { state: "not_needed" | "not_asked" | "unavailable" | "you" | "yours" }
  | { state: "yes" | "no"; by: EngineRef; reason: string }
  | { state: "reviewing"; by: EngineRef };

export interface GuardFacts {
  /** The command in a few words, for the dialog: "TRUNCATE audit_log". */
  short: string;
  target: string;
  /** "remote database", "bucket", "production-linked" */
  kind: string;
  /** The rule that marked the target, for example "prod-targets". */
  markedBy: string | null;
  /** The plain reason, for example "This command deletes data in a production-linked target." */
  reason: string;
  /** "16:02": when the held command is waiting since, or when it was decided or run. */
  at: string | null;
  exit: number | null;
}

export interface ApprovalRow {
  id: string;
  /** "14:12" in the person's time. */
  time: string;
  /** What was asked, in words: "Install zod 3.24 as a dev dependency". */
  title: string;
  asker: EngineRef;
  /** The task's branch, for example "crew/redirect-tests". */
  branch: string | null;
  risk: Risk | null;
  /** The row's own note: "inside its lease", "1 value hidden". */
  notes: string[];
  /** Labelled facts shown under "Asked by": For, Command, Target, Action, Why asked, The task. */
  facts: { label: string; text: string }[];
  /** The exact command in a block (secrets masked by the core), for a guarded command. */
  block: string | null;
  /** The row's own closing sentence: what the worker was told, and what it can do next. */
  note: string | null;
  /** For a request denied for want of a second key: the Needs you item's words. */
  needsYou: string | null;
  hard: HardKind;
  /** The rule that denied, in words, when `hard` is `denied`. */
  rule: string | null;
  lead: LeadAnswer;
  /** The engine that is the lead of the crew, for the side-by-side keys of a guarded command. */
  leadBy: EngineRef | null;
  second: SecondAnswer;
  result: ResultKind;
  /** Milliseconds the code took, for a hard rule. */
  decidedInMs: number | null;
  guard: GuardFacts | null;
}

export interface ApprovalCounts {
  all: number;
  allowed: number;
  denied_rule: number;
  denied_lead: number;
  keys_disagree: number;
  person_only: number;
}

export type ApprovalFilter = keyof ApprovalCounts;

export interface ApprovalPage {
  rows: ApprovalRow[];
  /** The real totals, whatever the page holds. */
  counts: ApprovalCounts;
  /** The rows the filter and search match. */
  total: number;
  offset: number;
  mode: Mode;
  /** The day the rows are from, as read: "Today · Sat 7 Nov 2026". */
  day: string | null;
}

export type StopReason = "too_many_denials" | "injected_instructions";

export interface DeniedRequest {
  time: string;
  title: string;
  why: string;
  result: "Denied by rule" | "Denied by lead";
}

export interface TripwireStop {
  taskId: string;
  taskTitle: string;
  engine: EngineRef;
  reason: StopReason;
  /** How many denials stopped it, and whether they were counted in a row (3) or in one run (20). */
  denialCount: number;
  inARun: boolean;
  denials: DeniedRequest[];
  /** For an injected-instructions stop: where the text was read and the text itself, shown as data. */
  injected: { where: string; source: string; text: string } | null;
}

export interface CrewPreview {
  enabled: boolean;
}

export interface HardDeniedRule {
  id: string;
  /** False while the rule is not enforced; the screen leaves the row out. */
  enforced: boolean;
}

export interface CrewOverride {
  crew: string;
  detail: string;
  mode: Mode;
  /** "You set this crew to On on Sat 7 Nov." or "Follows the repo." */
  why: string;
}

export interface AutopilotSettings {
  repo: string;
  mode: Mode;
  /** Set once the person has chosen or kept the default; false shows the first-run banner. */
  confirmed: boolean;
  crews: CrewOverride[];
  hardDenied: HardDeniedRule[];
  /** The second key's engines that are signed in now. */
  secondKeys: EngineRef[];
  prodTargets: TargetRule[];
  safeTargets: TargetRule[];
}

export interface TargetRule {
  name: string;
  kind: string;
  matchedBy: string[];
  source: "prod_targets" | "safe_targets";
}

export interface TrustCell {
  count: number;
  of: number;
}

export interface TrustRow {
  name: string;
  vendor: string | null;
  reverted: TrustCell;
  reworked: TrustCell;
  merges: number;
}

export interface ReviewerRow {
  name: string;
  confirmed: number;
  dropped: number;
  findings: number;
}

export interface SoloRow {
  taskClass: string;
  crew: { merges: number; reverted: number; reworked: number; medianMin: number; cost: string };
  solo: { runs: number; reverted: number; reworked: number; medianMin: number; cost: string };
  /** Words from the core's comparison; null when the sample cannot show it. */
  shows: string | null;
}

export interface TrustView {
  repo: string;
  days: number;
  merges: number;
  engines: TrustRow[];
  areas: TrustRow[];
  reviewers: ReviewerRow[];
  solo: SoloRow[];
}

export type TemplateId = "fix_ci" | "flaky_test" | "dependency_bump" | "security_advisory";
export type TriggerId = "ci_fails" | "issue_label" | "schedule" | "dependency_update";

export interface BackgroundCrew {
  id: string;
  name: string;
  template: TemplateId;
  trigger: TriggerId;
  startsWhen: string;
  lastRun: string | null;
  enabled: boolean;
  mode: Mode;
  budget: string;
}

export interface BackgroundRun {
  id: string;
  name: string;
  template: TemplateId;
  startedBy: string;
  result: string;
  state: "Merged" | "Running" | "Stopped" | "Failed";
  mode: Mode;
}

export interface BackgroundView {
  crews: BackgroundCrew[];
  runs: BackgroundRun[];
  /** The default budget for a new crew, from Settings. */
  defaultBudget: string;
}

export interface NewBackgroundCrew {
  template: TemplateId;
  trigger: TriggerId;
  detail: string;
  budget: string;
}

export interface StageLine {
  label: string;
  text: string;
}

/** The Flow panel for the "You" stage: what the person owes before or after the merge. */
export type YouState =
  | { kind: "after_merge"; mergedAt: string; risk: Risk; reviewBy: string; lines: StageLine[]; notVerified: string }
  | { kind: "rule"; rule: string; setBy: string; stages: string; mergedAt: string; text: string }
  | { kind: "blocked"; waiting: number; of: number; lines: StageLine[] }
  | { kind: "waiting_on_you"; risk: Risk; paths: string; size: string; lines: StageLine[]; notVerified: string }
  | { kind: "off"; risk: Risk; riskNote: string; lines: StageLine[] };

/** A command a guard holds, as Flow shows it under the Build node. */
export interface HeldCommand {
  command: string;
  reason: string;
  /** "The lead said yes." */
  key1: string;
  /** "Waiting on OpenCode · DeepSeek V4, a different vendor." */
  key2: string;
  /** Under On or Off the command is held for the person, not for a second key. */
  forYou: boolean;
}
