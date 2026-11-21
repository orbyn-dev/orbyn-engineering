import { Check, Loader2, Minus, X } from "lucide-react";
import { useState } from "react";
import "./codemode.css";
import { callDuration } from "../../lib/format";
import { sizeLabel } from "./model";
import type { CallStatus, CodeCall } from "./types";

/** A long list shows its first calls and one line for the rest, until the person asks for all (at most 200 in a run). */
const COLLAPSE_AT = 8;
const HEAD = 3;
const PREVIEW_CHARS = 160;

function Mark({ status }: { status: CallStatus }) {
  if (status === "running") return <Loader2 size={12} className="spin" role="img" aria-label="Running" />;
  if (status === "waiting") return <span className="status-dot" role="img" aria-label="Waiting for approval" />;
  if (status === "done") return <Check size={12} role="img" aria-label="Done" />;
  if (status === "interrupted" || status === "unknown") return <Minus size={12} role="img" aria-label={status === "unknown" ? "Unknown" : "Interrupted"} />;
  return <X size={12} role="img" aria-label="Did not run" />;
}

const SAY: Partial<Record<CallStatus, string>> = {
  declined: "Declined",
  expired: "Expired",
  refused: "Refused",
  interrupted: "Running when Orbyn restarted",
  failed: "Failed",
  unknown: "Unknown",
};

const rowClass = (status: CallStatus): string =>
  status === "done" ? "cm-ok" : status === "running" ? "cm-run" : status === "waiting" ? "cm-wait" : status === "declined" || status === "expired" ? "cm-declined" : status === "refused" || status === "failed" ? "cm-fail" : "cm-skip";

/** What a finished call came back with, cut to one line; the whole preview lives in the record, not the page. */
function Said({ call }: { call: CodeCall }) {
  const say = SAY[call.status];
  if (say) return <span className="cr">{say}</span>;
  if (call.preview === null) return <span className="cr" />;
  const flat = call.preview.replace(/\s+/g, " ").trim();
  const text = flat.length > PREVIEW_CHARS ? flat.slice(0, PREVIEW_CHARS) : flat;
  const kept = new TextEncoder().encode(call.preview).length;
  const partial = call.resultBytes !== null && call.resultBytes > kept;
  return (
    <span className="cr">
      <span className="cr-t">{text}</span>
      {partial && call.resultBytes !== null ? <span className="cr-n">… {sizeLabel(kept)} of {sizeLabel(call.resultBytes)}</span> : null}
    </span>
  );
}

function Row({ call }: { call: CodeCall }) {
  return (
    <div className={`cm-c ${rowClass(call.status)}`} role="group" aria-label={`Call ${call.index}, ${call.tool}`}>
      <span className="ci"><Mark status={call.status} /></span>
      <span className="cn">{call.index}</span>
      <span className="ct">{call.tool}</span>
      <Said call={call} />
      <span className="cd">{call.durationMs !== null ? callDuration(call.durationMs) : ""}</span>
    </div>
  );
}

/** The calls of a run, one line each. `title` is the heading ("Calls", "Calls so far"). */
export function CallList({ title, calls }: { title: string; calls: readonly CodeCall[] }) {
  const [all, setAll] = useState(false);
  const folded = !all && calls.length > COLLAPSE_AT;
  const shown = folded ? calls.slice(0, HEAD) : calls;
  const rest = calls.length - shown.length;
  return (
    <div className="cm-sec" role="group" aria-label={title}>
      <div className="cm-h">{title} · {calls.length}</div>
      {shown.map((call, at) => <Row key={`${at}-${call.index}`} call={call} />)}
      {folded ? (
        <div className="cm-c cm-skip">
          <span className="ci" />
          <span className="cn">{HEAD + 1}</span>
          <span className="ct">…</span>
          <button type="button" onClick={() => setAll(true)}>{rest} more calls · Show all</button>
          <span className="cd" />
        </div>
      ) : null}
    </div>
  );
}
