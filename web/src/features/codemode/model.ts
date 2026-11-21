import { callDuration } from "../../lib/format";
import type { Approval } from "../../api/types";
import type { ApprovalVia, CallStatus, CodeCall, CodeErrorKind, CodeRunError, CodeReply, CodeRunCallWire, CodeRunRecord, CodeRunStatus, RunView } from "./types";

export const CODE_TOOLS = ["orbyn_code_search", "orbyn_code_describe", "orbyn_code_run"] as const;
export type CodeTool = "search" | "describe" | "run";

/** The limits docs/code-mode.md gives; the screens never draw more than these, so no run can grow the page. */
export const LIMITS = { logLines: 500, logLineChars: 2_000, calls: 200, callPreviewChars: 4_096, resultChars: 65_536, scriptPage: 400 };

/** The three tools of Orbyn's own MCP server, from an engine's name for the call. Another server's tool of the same name is not Orbyn's. */
export function codeToolOf(name: string): CodeTool | null {
  const parts = name.split(/__|[.:]/).filter(Boolean);
  if (parts.length < 3 || parts[1] !== "orbyn_engineering") return null;
  const tool = parts.slice(2).join("_");
  return tool === "orbyn_code_run" ? "run" : tool === "orbyn_code_search" ? "search" : tool === "orbyn_code_describe" ? "describe" : null;
}

/** True for one of the three tools by its bare name (the Tools tab lists them that way). */
export const isCodeToolName = (name: string): boolean => (CODE_TOOLS as readonly string[]).includes(name);

const byteLength = (text: string) => new TextEncoder().encode(text).length;

/** "412 B", "38 KiB". */
export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return `${Math.round(bytes / 1024)} KiB`;
}

export interface ScriptMeta {
  lines: number;
  bytes: number;
  header: string;
}

/** The lines of a script; a trailing newline does not start another line. */
export const scriptLines = (script: string): string[] => (script === "" ? [] : script.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n"));

/** "Script · 12 lines · 412 B". */
export function scriptMeta(script: string): ScriptMeta {
  const lines = scriptLines(script).length;
  const bytes = byteLength(script);
  return { lines, bytes, header: `Script · ${lines} ${lines === 1 ? "line" : "lines"} · ${sizeLabel(bytes)}` };
}

export const shortHash = (hex: string): string => (hex.length > 12 ? `${hex.slice(0, 6)}…${hex.slice(-3)}` : hex);

/** The newest `max` lines, each cut at `chars`, and how many older lines were left out. */
export function boundedLines(lines: readonly string[], max: number, chars: number = LIMITS.logLineChars): { shown: string[]; hidden: number } {
  const kept = lines.slice(-max);
  return { shown: kept.map((line) => (line.length > chars ? `${line.slice(0, chars)}…` : line)), hidden: lines.length - kept.length };
}

/** Text a model or script controls is cut before it is drawn: `…` marks the cut. */
export const capText = (text: string, max: number): string => (text.length > max ? `${text.slice(0, max)}…` : text);

export const MAX_SCRIPT_CHARS = 65_536;
const MAX_MESSAGE_CHARS = 4_000;

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

/** The approval's `via`, or null. Display only: nothing that decides an approval reads it. */
export function viaOf(approval: Approval): ApprovalVia | null {
  const via = (approval as Approval & { via?: unknown }).via;
  if (!isRecord(via)) return null;
  const { code_run: run, call, script_sha256: sha } = via;
  if (typeof run !== "string" || run === "" || typeof call !== "number" || !Number.isInteger(call)) return null;
  return { codeRun: run, call, scriptSha256: typeof sha === "string" ? sha : null };
}

const CALL_WORDS = new Map<string, CallStatus>(Object.entries({
  running: "running",
  started: "running",
  waiting: "waiting",
  held: "waiting",
  pending: "waiting",
  waiting_approval: "waiting",
  waiting_for_approval: "waiting",
  ok: "done",
  done: "done",
  completed: "done",
  success: "done",
  error: "failed",
  failed: "failed",
  failure: "failed",
  denied: "declined",
  declined: "declined",
  expired: "expired",
  timed_out: "expired",
  invalid: "refused",
  refused: "refused",
  in_flight: "interrupted",
  interrupted: "interrupted",
  lost: "interrupted",
}));

/** A call from the record. A status this file does not know is "unknown", never a success. */
export function normalizeCall(call: CodeRunCallWire): CodeCall {
  return {
    index: call.index,
    tool: call.tool,
    status: (typeof call.status === "string" ? CALL_WORDS.get(call.status) : undefined) ?? "unknown",
    preview: typeof call.preview === "string" ? call.preview.slice(0, LIMITS.callPreviewChars) : null,
    resultBytes: typeof call.result_bytes === "number" ? call.result_bytes : null,
    durationMs: typeof call.duration_ms === "number" ? call.duration_ms : null,
  };
}

const KINDS: readonly CodeErrorKind[] = ["syntax", "runtime", "limit", "denied", "lost"];
const STATUSES: readonly CodeRunStatus[] = ["running", "paused", "completed", "error"];

/** The reply an engine read from `orbyn_code_run`, or null when the text is not one. Never throws. */
export function readReply(text: string | null): CodeReply | null {
  if (!text) return null;
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isRecord(value)) return null;
  const status = STATUSES.find((known) => known === value.status);
  if (!status) return null;
  const reply: CodeReply = { status };
  if ("result" in value) reply.result = value.result;
  if (Array.isArray(value.logs)) reply.logs = value.logs.filter((line): line is string => typeof line === "string");
  if (typeof value.runId === "string") reply.runId = value.runId;
  if (value.truncated === true) reply.truncated = true;
  if (Array.isArray(value.pending)) {
    reply.pending = value.pending.flatMap((entry) => (isRecord(entry) && typeof entry.tool === "string" && typeof entry.call === "number" ? [{ tool: entry.tool, call: entry.call }] : []));
  }
  if (isRecord(value.error) && typeof value.error.message === "string") {
    // A kind this file does not know is shown as the plain runtime error it most likely is.
    const kind = KINDS.find((known) => known === (value.error as Record<string, unknown>).kind) ?? "runtime";
    const line = (value.error as Record<string, unknown>).line;
    reply.error = typeof line === "number" ? { kind, message: value.error.message, line } : { kind, message: value.error.message };
  }
  return reply;
}

/** What the block draws: the run record when it was read, otherwise only what the engine's reply carried. */
export function runView(record: CodeRunRecord | null, reply: CodeReply | null, script: string | null = null): RunView | null {
  const cap = (value: string | null) => (value === null ? null : value.slice(0, MAX_SCRIPT_CHARS));
  const fit = (error: CodeRunError | null | undefined): CodeRunError | null =>
    error ? { ...error, kind: KINDS.find((known) => known === error.kind) ?? "runtime", message: capText(String(error.message), MAX_MESSAGE_CHARS) } : null;
  if (record) {
    return {
      runId: record.run_id,
      status: record.status,
      script: cap(record.script ?? script),
      scriptSha256: record.script_sha256 ?? null,
      startedAt: record.started_at,
      endedAt: record.ended_at,
      calls: (Array.isArray(record.calls) ? record.calls : []).slice(0, LIMITS.calls).map(normalizeCall),
      logs: record.logs ?? reply?.logs ?? [],
      logsTotal: record.logs_total ?? null,
      result: record.result !== undefined ? record.result : reply?.result,
      hasResult: record.result !== undefined || (reply !== null && "result" in reply),
      error: fit(record.error ?? reply?.error),
      pending: record.pending ?? reply?.pending ?? [],
      truncated: record.truncated === true || reply?.truncated === true,
      resumes: record.resumes ?? 0,
      waitedMs: record.waited_ms ?? null,
      crewWorker: record.crew_worker === true,
      hasRecord: true,
    };
  }
  if (!reply) return null;
  return {
    runId: reply.runId ?? null,
    status: reply.status,
    script: cap(script),
    scriptSha256: null,
    startedAt: null,
    endedAt: null,
    calls: [],
    logs: reply.logs ?? [],
    logsTotal: null,
    result: reply.result,
    hasResult: "result" in reply,
    error: fit(reply.error),
    pending: reply.pending ?? [],
    truncated: reply.truncated === true,
    resumes: 0,
    waitedMs: null,
    crewWorker: false,
    hasRecord: false,
  };
}

export type RowTone = "run" | "done" | "wait" | "fail";

const ERROR_NOTES = new Map<string, string>(Object.entries({
  syntax: "· syntax error",
  runtime: "· runtime error",
  limit: "· limit",
  denied: "· denied",
  lost: "· core restarted",
}));

export const callWord = (count: number): string => `${count} ${count === 1 ? "call" : "calls"}`;

/** How long the run has gone, or went. Null when there is no clock. */
export function runMs(view: RunView, now: number): number | null {
  if (!view.startedAt) return null;
  const start = Date.parse(view.startedAt);
  if (Number.isNaN(start)) return null;
  const end = view.endedAt ? Date.parse(view.endedAt) : now;
  return Number.isNaN(end) ? null : Math.max(0, end - start);
}

/** The words the row carries: the note after the tool name, the right-hand slot, and the tone of the mark. */
export function rowSummary(view: RunView, now: number): { note: string; due: string; tone: RowTone } {
  const ms = runMs(view, now);
  const time = ms === null ? "" : callDuration(ms);
  const calls = view.calls.length;
  if (view.status === "error") {
    const kind = view.error?.kind ?? "runtime";
    return { note: ERROR_NOTES.get(kind) ?? "· runtime error", due: kind === "lost" ? "Lost" : "Failed", tone: "fail" };
  }
  if (view.status === "paused") {
    const finished = view.calls.filter((call) => call.status !== "waiting" && call.status !== "running").length;
    return { note: `· ${finished} ${finished === 1 ? "call" : "calls"} done`, due: "Paused", tone: "wait" };
  }
  // A run still going counts whole seconds, as the frames do; a finished one keeps the call rows' tenths.
  if (view.status === "running") return { note: view.resumes > 0 ? "· resumed" : `· ${calls} ${calls === 1 ? "call" : "calls"} so far`, due: ms === null ? "" : ms < 60_000 ? `${Math.round(ms / 1000)}s` : time, tone: "run" };
  return { note: `· ${callWord(calls)}`, due: time, tone: "done" };
}

/** "resumed once", "resumed 3 times". */
export const resumedWords = (resumes: number): string => (resumes === 1 ? "resumed once" : `resumed ${resumes} times`);
