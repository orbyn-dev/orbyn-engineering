import { Wrench } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import "./codemode.css";
import type { Approval, JsonValue } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { CallList } from "./CallList";
import { callsOf, isRunId, viaOf } from "./model";
import { openRun } from "./openRun";
import { ScriptBox } from "./ScriptBox";
import { useCodeRun } from "./useCodeRun";
import type { ApprovalVia } from "./types";

/**
 * The parts of an approval card that belong to an inner call of a Code Mode script. Everything here is DISPLAY ONLY
 * (docs/code-mode.md, "Approvals show the script"): the card's buttons, its decisions and its rules never read `via`, and
 * nothing in this file writes. The script and the calls come from the run record and are shown as text.
 */

/**
 * The tool the approval is for, from the approval's own tool field (`detail.tool` or `detail.target`), never from its summary,
 * which a model can influence. Null when the approval does not say: the card then says "this tool".
 */
export function toolOfApproval(approval: Approval): string | null {
  const detail = approval.detail;
  if (typeof detail !== "object" || detail === null || Array.isArray(detail)) return null;
  for (const key of ["tool", "target"]) {
    const value = detail[key];
    if (typeof value === "string" && /^[A-Za-z0-9_.:-]{1,80}$/.test(value)) return value;
  }
  return null;
}

const callTitle = (approval: Approval): string => {
  const tool = toolOfApproval(approval);
  return tool ? `Use ${tool}` : approval.summary;
};

/** Where an approval stands among the pending approvals of the same run: first is open, the rest wait their turn. */
export function useViaQueue(approval: Approval, via: ApprovalVia | null): { index: number; total: number } {
  const pending = useWorkspace((state) => state.approvals.data);
  if (!via || !pending) return { index: 0, total: 1 };
  const mine = pending
    .filter((other) => other.state === "pending" && viaOf(other)?.codeRun === via.codeRun)
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id));
  const at = mine.findIndex((other) => other.id === approval.id);
  return at < 0 ? { index: 0, total: 1 } : { index: at, total: mine.length };
}

const MAX_ROWS = 8;
const MAX_VALUE = 400;
const INDEX_KEY = /^(0|[1-9]\d*)$/;

/** One argument as a short line. A list or an object is summarised, never dropped; a cut string ends in an ellipsis. */
function shown(value: JsonValue): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return `list of ${value.length}`;
  if (typeof value === "object") {
    const count = Object.keys(value).length;
    return `object with ${count} ${count === 1 ? "field" : "fields"}`;
  }
  const text = String(value);
  return text.length > MAX_VALUE ? `${text.slice(0, MAX_VALUE)}…` : text;
}

/**
 * The tool's own arguments as rows. Named fields come before numeric keys (a JS object lists those first), at most eight
 * rows show, and a line says how many more there are, so the card never looks complete when it is not.
 */
function factRows(detail: JsonValue): { rows: [string, string][]; more: number } {
  if (typeof detail !== "object" || detail === null || Array.isArray(detail)) return { rows: [], more: 0 };
  const entries = Object.entries(detail);
  const ordered = [...entries.filter(([key]) => !INDEX_KEY.test(key)), ...entries.filter(([key]) => INDEX_KEY.test(key))];
  return { rows: ordered.slice(0, MAX_ROWS).map(([key, value]) => [key.slice(0, 80), shown(value)]), more: Math.max(0, ordered.length - MAX_ROWS) };
}

/**
 * "Open the run": goes to the approval's own thread and scrolls to the run's block. Drawn only for a usable run id; one click, no
 * writes. When the block is not in the transcript (the record expired, or the thread has not loaded it) it says so quietly.
 */
function OpenRun({ threadId, codeRun }: { threadId: string; codeRun: string }) {
  const [missing, setMissing] = useState(false);
  const cancel = useRef<(() => void) | null>(null);
  useEffect(() => () => cancel.current?.(), []);
  if (!isRunId(codeRun)) return null;
  return (
    <p className="approval-kv cm-open">
      <button
        type="button"
        className="cm-link"
        onClick={() => {
          cancel.current?.();
          setMissing(false);
          cancel.current = openRun(threadId, codeRun, (found) => setMissing(!found));
        }}
      >
        Open the run
      </button>
      {missing ? <span className="cm-open-note" role="status">That run is not in this thread's transcript.</span> : null}
    </p>
  );
}

/** What the call is: its tool, then its arguments, then where it came from. */
export function ViaFacts({ approval, via, withTool = true }: { approval: Approval; via: ApprovalVia; withTool?: boolean }) {
  const { rows, more } = factRows(approval.detail);
  return (
    <>
      {withTool ? (
        <div className="approval-targets">
          <p className="approval-target">
            <Wrench size={14} aria-hidden />
            <code>{callTitle(approval)}</code>
          </p>
        </div>
      ) : null}
      {rows.map(([key, value], at) => (
        <p key={`${at}-${key}`} className="approval-kv">
          <span>{key}</span>
          <b className="mono">{value}</b>
        </p>
      ))}
      {more > 0 ? (
        <p className="approval-kv">
          <span>…</span>
          <b>{more} more {more === 1 ? "field" : "fields"}</b>
        </p>
      ) : null}
      <p className="approval-kv">
        <span>From</span>
        <b>A Code Mode script · call {via.call}</b>
      </p>
      <OpenRun threadId={approval.thread_id} codeRun={via.codeRun} />
    </>
  );
}

/** The script as written, and the calls made so far, read from the run record while the approval waits. */
export function ViaContext({ via }: { via: ApprovalVia }) {
  const record = useCodeRun(via.codeRun, true);
  if (!record) return null;
  const made = callsOf(record).filter((call) => call.index !== via.call && call.status !== "running" && call.status !== "waiting");
  return (
    <>
      {typeof record.script === "string" ? <ScriptBox script={record.script} sha={via.scriptSha256 ?? record.script_sha256 ?? null} /> : null}
      {made.length > 0 ? <CallList title="Calls so far" calls={made} /> : null}
    </>
  );
}

/** The line under the buttons: Always allow is about the tool, never about the script. */
export function ViaAlways({ approval }: { approval: Approval }) {
  return <p className="cm-always">Always allow covers {toolOfApproval(approval) ?? "this tool"}, not this script.</p>;
}

/** An answered approval: one quiet block with the tool, the answer, and what the script saw. */
export function ViaDone({ approval, via }: { approval: Approval; via: ApprovalVia }) {
  const allowed = approval.state === "allowed";
  return (
    <article className="cm-done" aria-label={approval.summary}>
      <div className="cm-tool">
        <Wrench size={14} aria-hidden />
        <span>{callTitle(approval)}</span>
        <span className="cm-callno">call {via.call}</span>
      </div>
      <p className="cm-res">{allowed ? "Approved" : "Declined"}</p>
      <p className="cm-sub">{allowed ? "The call ran. The script continues." : "The script got a refusal: denied."}</p>
    </article>
  );
}

/** An approval nobody answered in time: grey, no buttons, and what happened. */
export function ViaClosed({ approval, via }: { approval: Approval; via: ApprovalVia }) {
  return (
    <article className="approval cm-closed cm-apr" aria-label={approval.summary}>
      <div className="approval-top">
        <p className="ask-label">
          <span className="status-dot" aria-hidden />
          Request closed
        </p>
        <span className="approval-wait">Expired</span>
      </div>
      <ViaFacts approval={approval} via={via} />
      <p className="cm-banner">Nobody answered in time. The call did not run; the script got a refusal.</p>
    </article>
  );
}
