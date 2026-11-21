import { Wrench } from "lucide-react";
import "./codemode.css";
import type { Approval, JsonValue } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { CallList } from "./CallList";
import { normalizeCall, viaOf } from "./model";
import { ScriptBox } from "./ScriptBox";
import { useCodeRun } from "./useCodeRun";
import type { ApprovalVia } from "./types";

/**
 * The parts of an approval card that belong to an inner call of a Code Mode script. Everything here is DISPLAY ONLY
 * (docs/code-mode.md, "Approvals show the script"): the card's buttons, its decisions and its rules never read `via`, and
 * nothing in this file writes. The script and the calls come from the run record and are shown as text.
 */

/** The tool the approval is for, from its "Use <tool>" summary. */
export const toolOfApproval = (approval: Approval): string => approval.summary.replace(/^Use\s+/, "");

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

/** The tool's own arguments as rows, when the approval's detail carries them as plain values. */
function factRows(detail: JsonValue): [string, string][] {
  if (typeof detail !== "object" || detail === null || Array.isArray(detail)) return [];
  return Object.entries(detail)
    .flatMap(([key, value]): [string, string][] => (typeof value === "string" || typeof value === "number" || typeof value === "boolean" ? [[key, String(value).slice(0, 400)]] : []))
    .slice(0, 8);
}

/** What the call is: its tool, then its arguments, then where it came from. */
export function ViaFacts({ approval, via, withTool = true }: { approval: Approval; via: ApprovalVia; withTool?: boolean }) {
  const rows = factRows(approval.detail);
  return (
    <>
      {withTool ? (
        <div className="approval-targets">
          <p className="approval-target">
            <Wrench size={14} aria-hidden />
            <code>{approval.summary}</code>
          </p>
        </div>
      ) : null}
      {rows.map(([key, value]) => (
        <p key={key} className="approval-kv">
          <span>{key}</span>
          <b className="mono">{value}</b>
        </p>
      ))}
      <p className="approval-kv">
        <span>From</span>
        <b>A Code Mode script · call {via.call}</b>
      </p>
    </>
  );
}

/** The script as written, and the calls made so far, read from the run record while the approval waits. */
export function ViaContext({ via }: { via: ApprovalVia }) {
  const record = useCodeRun(via.codeRun, true);
  if (!record) return null;
  const made = record.calls.map(normalizeCall).filter((call) => call.index !== via.call && call.status !== "running" && call.status !== "waiting");
  return (
    <>
      {record.script !== null ? <ScriptBox script={record.script} sha={via.scriptSha256 ?? record.script_sha256 ?? null} /> : null}
      {made.length > 0 ? <CallList title="Calls so far" calls={made} /> : null}
    </>
  );
}

/** The line under the buttons: Always allow is about the tool, never about the script. */
export function ViaAlways({ approval }: { approval: Approval }) {
  return <p className="cm-always">Always allow covers {toolOfApproval(approval)} only, never this script.</p>;
}

/** An approval that waits behind another call of the same run: one quiet line, opened when the one before it is answered. */
export function ViaNext({ approval, via }: { approval: Approval; via: ApprovalVia }) {
  return (
    <div className="cm-next">
      <span>Next: {toolOfApproval(approval)} · call {via.call}</span>
      <span className="t3">waiting</span>
    </div>
  );
}

/** An answered approval: one quiet block with the tool, the answer, and what the script saw. */
export function ViaDone({ approval, via }: { approval: Approval; via: ApprovalVia }) {
  const allowed = approval.state === "allowed";
  return (
    <article className="cm-done" aria-label={approval.summary}>
      <div className="cm-tool">
        <Wrench size={14} aria-hidden />
        <span>{approval.summary}</span>
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
