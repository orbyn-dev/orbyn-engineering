// What the Autopilot screens draw. The wire shapes are the generated `Autopilot*` types; `ledger.ts` maps a
// ledger row to the row the log draws. Guard rows (production-linked targets) have no route yet, so only
// the components and their tests build them.
import type { AutopilotMode } from "../../../../generated/AutopilotMode";

export type Mode = AutopilotMode;
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
  /** The ledger row's own time, for the day line. */
  at: string | null;
  /** What was asked, in words: "Install zod 3.24 as a dev dependency". */
  title: string;
  /** The engine that asked, when the ledger row says; the stored row names the task only. */
  asker: EngineRef | null;
  taskId: string | null;
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

export type ApprovalFilter = "all" | "allowed" | "denied_rule" | "denied_lead" | "keys_disagree" | "person_only";

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
  engine: EngineRef | null;
  reason: StopReason;
  /** How many denials stopped it, and whether they were counted in a row (3) or in one run (20). */
  denialCount: number;
  inARun: boolean;
  denials: DeniedRequest[];
  /** For an injected-instructions stop: where the text was read and the text itself, shown as data. */
  injected: { where: string; source: string; text: string } | null;
}

export type TemplateId = "fix_ci" | "flaky_test" | "dependency_bump" | "security_advisory";
export type TriggerId = "ci_fails" | "issue_label" | "schedule" | "dependency_update";

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
