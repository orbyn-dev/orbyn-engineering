/**
 * What the Code Mode screens read. The wire types are the generated ones (`web/src/generated`, from `orbyn-contracts`), with
 * every i64 as the number `JSON.parse` delivers (`Wired`). `model.ts` is the one place that reads them: it narrows the free
 * strings (`status` of a call, `kind` of an error) and bounds every size. The view types below are what the blocks draw.
 *
 * Differences from the first hand-written guess, found when the routes landed:
 * - `script`, `ended_at` and the three counters are optional or nullable as the contract says (`script?`, `ended_at?`).
 * - `logs`, `pending`, `truncated`, `resumes` and `crew_worker` are always present.
 * - A call's `status` is only `running`, `ok`, `failed` or `denied`. There is no "waiting": a call held for a person is a `running`
 *   call of a `paused` run that `pending` names, and a call a restart cut off stays `running` in a run that ended as `error`.
 * - `error.kind` is a plain string on the wire, and `result` over 64 KiB is `{ truncated, bytes, preview }`.
 * - `waited_ms` is always null for now. The approval's `via` is not on the wire: it stays display-only (see `ApprovalVia`).
 */
import type { CodeModeSetting as CodeModeSettingWire } from "../../generated/CodeModeSetting";
import type { CodeRunCall as CodeRunCallWire } from "../../generated/CodeRunCall";
import type { CodeRunList as CodeRunListWire } from "../../generated/CodeRunList";
import type { CodeRunRecord as CodeRunRecordWire } from "../../generated/CodeRunRecord";
import type { CodeRunSummary as CodeRunSummaryWire } from "../../generated/CodeRunSummary";
import type { Wired } from "../../api/types";

import type { CodeRunStatus } from "../../generated/CodeRunStatus";

export type { CodeRunStatus };

export type CodeRunRecord = Wired<CodeRunRecordWire>;
export type CodeRunCall = Wired<CodeRunCallWire>;
export type CodeRunSummary = Wired<CodeRunSummaryWire>;
export type CodeRunList = Wired<CodeRunListWire>;
export type CodeModeSetting = Wired<CodeModeSettingWire>;

export type CodeErrorKind = "syntax" | "runtime" | "limit" | "denied" | "lost";

/** An error as the blocks draw it: the kind is one of the five, whatever the wire sent. */
export interface RunError {
  kind: CodeErrorKind;
  message: string;
  line?: number | null;
}

/** The JSON the engine read back from `orbyn_code_run`. */
export interface CodeReply {
  status: CodeRunStatus;
  result?: unknown;
  logs?: string[];
  runId?: string;
  error?: RunError;
  pending?: { tool: string; call: number }[];
  truncated?: boolean;
}

export type CallStatus = "running" | "waiting" | "done" | "failed" | "declined" | "expired" | "refused" | "interrupted" | "unknown";

export interface CodeCall {
  index: number;
  tool: string;
  status: CallStatus;
  preview: string | null;
  resultBytes: number | null;
  durationMs: number | null;
}

/** Everything one run block draws, from the record when there is one and from the reply otherwise. */
export interface RunView {
  runId: string | null;
  status: CodeRunStatus;
  script: string | null;
  scriptSha256: string | null;
  startedAt: string | null;
  endedAt: string | null;
  calls: CodeCall[];
  logs: string[];
  logsTotal: number | null;
  result: unknown;
  hasResult: boolean;
  error: RunError | null;
  pending: { tool: string; call: number }[];
  truncated: boolean;
  resumes: number;
  waitedMs: number | null;
  crewWorker: boolean;
  /** True when the run record was read. Without it the block has no call list. */
  hasRecord: boolean;
}

/** The display-only `via` an approval carries when an inner call of a script asked for it. */
export interface ApprovalVia {
  codeRun: string;
  call: number;
  scriptSha256: string | null;
}
