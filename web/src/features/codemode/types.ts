/**
 * What the Code Mode screens read. docs/code-mode.md is the contract; the wire shapes below follow its "Run record" and
 * "orbyn_code_run" sections. They are NOT in crates/orbyn-contracts or web/src/generated yet (the backend lands in its own
 * pull requests), so these types are written by hand and every field the document does not name is optional. When the
 * generated types arrive, replace this file with imports and keep `model.ts` as the one place that reads them.
 */

export type CodeErrorKind = "syntax" | "runtime" | "limit" | "denied" | "lost";
export type CodeRunStatus = "running" | "paused" | "completed" | "error";

export interface CodeRunError {
  kind: CodeErrorKind;
  message: string;
  line?: number | null;
}

/** One inner call as the run record stores it. `status` is a free word on the wire; `normalizeCall` reads it. */
export interface CodeRunCallWire {
  index: number;
  tool: string;
  status: string;
  /** The first 4 KiB of the result, as text. */
  preview?: string | null;
  /** The size of the whole result, when the preview is a part of it. */
  result_bytes?: number | null;
  duration_ms?: number | null;
}

/** `code_run.get(runId)`: the run record. Records are kept 7 days. */
export interface CodeRunRecord {
  run_id: string;
  status: CodeRunStatus;
  /** The script text, at most 64 KiB. Untrusted: it is shown as text and never run or linked. */
  script: string | null;
  script_sha256?: string | null;
  started_at: string;
  ended_at: string | null;
  calls: CodeRunCallWire[];
  logs?: string[];
  /** How many lines the script printed in all, when the record kept only the newest. */
  logs_total?: number | null;
  /** The script's return value, as JSON. */
  result?: unknown;
  error?: CodeRunError | null;
  pending?: { tool: string; call: number }[];
  truncated?: boolean;
  /** How many times an engine call picked the run up again after it paused. */
  resumes?: number;
  /** Milliseconds the run waited for a person. */
  waited_ms?: number | null;
  /** The run belongs to a Crew worker's thread. */
  crew_worker?: boolean;
}

/** The JSON the engine read back from `orbyn_code_run`. */
export interface CodeReply {
  status: CodeRunStatus;
  result?: unknown;
  logs?: string[];
  runId?: string;
  error?: CodeRunError;
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
  error: CodeRunError | null;
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

/**
 * GUESSED SHAPE, not in docs/code-mode.md or any generated type: one row of the per-thread list of runs. The document says only that
 * the list shows "run id, status, start and end, call count and the first 120 characters of the script", and that the error kind
 * belongs to the run, not the list.
 */
export interface CodeRunSummary {
  run_id: string;
  status: string;
  started_at: string;
  ended_at: string | null;
  /** How many calls the run made. */
  calls: number;
  /** The first 120 characters of the script, when the service sends them. */
  script_head?: string | null;
}
