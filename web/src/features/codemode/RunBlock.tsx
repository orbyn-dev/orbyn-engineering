import { Loader2 } from "lucide-react";
import { useMemo, type ReactNode } from "react";
import "./codemode.css";
import { callDuration } from "../../lib/format";
import { useNow } from "../../lib/useNow";
import { parsed } from "../../product/timeline/toolTrace";
import type { ToolItem } from "../../product/timeline/types";
import { CallList } from "./CallList";
import { boundedLines, callWord, codeToolOf, LIMITS, readReply, resumedWords, rowSummary, runMs, runView, type RowTone } from "./model";
import { ScriptBox } from "./ScriptBox";
import { useCodeRun } from "./useCodeRun";
import type { RunView } from "./types";

/**
 * The run behind one `orbyn_code_run` call: the run record when it can be read (a dev or test build with the stand-in
 * service), otherwise only what the engine's own reply carried. The call's arguments supply the script when the record does
 * not; a call that only resumes (`run_id`) has none.
 */
export function useRunView(item: ToolItem, live: boolean): RunView | null {
  // Every other tool's result stays unparsed: only a run call's reply is read as one.
  const isRun = codeToolOf(item.name) === "run";
  const reply = useMemo(() => (isRun ? readReply(item.result) : null), [isRun, item.result]);
  const args = useMemo(() => (isRun ? parsed(item.args) : {}), [isRun, item.args]);
  const code = typeof args.code === "string" ? args.code : null;
  const runId = reply?.runId ?? (typeof args.run_id === "string" ? args.run_id : null);
  const record = useCodeRun(isRun ? runId : null);
  return useMemo(() => {
    if (!isRun) return null;
    const made = runView(record, reply, code);
    if (made) return made;
    // The call has no reply yet: the run is starting, and the script is known from the arguments.
    if (!item.done && item.result === null && live) return { ...runView(null, { status: "running" }, code)!, startedAt: item.ts };
    return null;
  }, [isRun, record, reply, code, item.done, item.result, item.ts, live]);
}

/** The row's words for a code run; null while the run is unknown, which leaves the generic row. */
export function useRunRow(item: ToolItem, live: boolean): { note: string; due: string; tone: RowTone } | null {
  const view = useRunView(item, live);
  const now = useNow(view?.status === "running");
  return view ? rowSummary(view, now) : null;
}

function Section({ title, note, children, error }: { title: ReactNode; note?: string | null; children: ReactNode; error?: boolean }) {
  return (
    <div className={`cm-sec${error ? " cm-err" : ""}`}>
      <div className="cm-h">{title}</div>
      {note ? <div className="cm-note">{note}</div> : null}
      {children}
    </div>
  );
}

const Quiet = ({ children }: { children: ReactNode }) => <div className="co mute">{children}</div>;

function resultText(view: RunView): string {
  let text: string;
  try {
    text = JSON.stringify(view.result === undefined ? null : view.result) ?? "null";
  } catch {
    text = "null";
  }
  return text.length > LIMITS.resultChars ? text.slice(0, LIMITS.resultChars) : text;
}

const isCutResult = (value: unknown) => typeof value === "object" && value !== null && (value as { truncated?: unknown }).truncated === true;

function WaitingOn({ view }: { view: RunView }) {
  const rows = view.pending;
  return (
    <div className="cm-sec cm-pend" role="group" aria-label="Waiting on">
      <div className="cm-h">Waiting on · {rows.length}</div>
      {rows.map((entry) => {
        const held = view.calls.find((call) => call.index === entry.call)?.status;
        return (
          <div key={entry.call} className="cm-c cm-wait">
            <span className="ci"><span className="status-dot" aria-hidden /></span>
            <span className="cn">{entry.call}</span>
            <span className="ct">{entry.tool}</span>
            <span className="cr">call {entry.call} · {held === "running" ? "running" : "waiting for approval"}</span>
            <span className="cd" />
          </div>
        );
      })}
    </div>
  );
}

function Footer({ view, now, inspect }: { view: RunView; now: number; inspect: ReactNode }) {
  const ms = runMs(view, now);
  const time = ms === null ? null : ms < 60_000 && view.status === "running" ? `${Math.round(ms / 1000)}s` : callDuration(ms);
  const calls = view.calls.length;
  const parts: ReactNode[] = [];
  const add = (text: string | null) => { if (text) parts.push(text); };
  if (view.status === "running") {
    parts.push(<span key="run"><Loader2 size={11} className="spin" aria-hidden /> Running</span>);
    add(time);
    if (view.resumes > 0) add(resumedWords(view.resumes));
  } else if (view.status === "paused") {
    parts.push("Paused");
    add(time);
  } else if (view.status === "completed") {
    parts.push("Completed");
    add(time);
    if (view.hasRecord) add(callWord(calls));
    if (view.waitedMs !== null && view.waitedMs > 0) add(`waited ${callDuration(view.waitedMs)} for you`);
  } else if (view.error?.kind === "lost") {
    parts.push("Lost");
    // A call that was in flight when the core restarted is not a recorded call.
    const recorded = view.calls.filter((call) => call.status !== "interrupted").length;
    if (view.hasRecord) add(`${recorded} ${recorded === 1 ? "call" : "calls"} recorded`);
  } else {
    parts.push("Failed");
    if (view.hasRecord) add(callWord(calls));
  }
  const settled = view.status === "completed" || view.status === "error";
  return (
    <div className="cf">
      <span className="tfm" data-find-skip>
        {parts.map((part, at) => (
          <span key={at}>
            {at > 0 ? <span className="dot"> · </span> : null}
            {part}
          </span>
        ))}
      </span>
      {settled ? inspect : null}
    </div>
  );
}

/** What the block opens: the script, the calls, the logs, the result or the error, and a footer line. Nothing here is HTML or a link. */
export function CodeRunBlock({ item, live, inspect }: { item: ToolItem; live: boolean; inspect: ReactNode }) {
  const view = useRunView(item, live);
  const now = useNow(view?.status === "running");
  if (!view) {
    // Not a reply this screen understands: show the arguments' script and the raw text, as plain text.
    const args = parsed(item.args);
    const code = typeof args.code === "string" ? args.code : null;
    return (
      <div className="cb">
        {code ? <ScriptBox script={code} /> : null}
        <pre className="co" tabIndex={0}>{(item.result ?? item.output ?? "").slice(0, 4_000)}</pre>
        <div className="cf"><span className="tfm" />{inspect}</div>
      </div>
    );
  }
  const logs = boundedLines(view.logs, LIMITS.logLines);
  // A reply cut for size sets `truncated` too, so it only means dropped log lines when the log is full.
  const lostLogs = logs.hidden > 0 || (view.logsTotal !== null && view.logsTotal > logs.shown.length) || (view.truncated && logs.shown.length >= LIMITS.logLines);
  const total = view.logsTotal ?? view.logs.length;
  const showScript = view.status !== "paused" && view.script !== null;
  const showLogs = view.status !== "paused" || logs.shown.length > 0;
  const suffix =
    view.status === "paused" ? " · paused"
    : view.status === "running" && view.resumes > 0 ? " · resumed after approval"
    : view.status === "completed" && view.resumes > 0 ? ` · ${resumedWords(view.resumes)}`
    : "";
  const error = view.error;
  return (
    <div className="cb" data-run-status={view.status}>
      {view.runId ? (
        <div className="cm">
          <span className="pr" aria-hidden />
          <span className="cx sans">
            Run ID <span className="cm-id">{view.runId}</span>
            {suffix}
            {view.crewWorker ? " · Crew worker" : ""}
          </span>
        </div>
      ) : null}
      {showScript && view.script !== null ? <ScriptBox script={view.script} sha={view.scriptSha256} defaultOpen={error?.kind === "syntax"} /> : null}
      {view.status === "paused" ? <WaitingOn view={view} /> : view.hasRecord ? <CallList title="Calls" calls={view.calls} /> : null}
      {showLogs ? (
        <Section title={`Logs · ${logs.shown.length}`} note={lostLogs ? `Older lines were dropped. Showing the last ${logs.shown.length.toLocaleString("en-US")}${view.logsTotal !== null ? ` of ${total.toLocaleString("en-US")}` : ""}.` : null}>
          {logs.shown.length === 0 ? <Quiet>No output.</Quiet> : <pre className="co scr tail" tabIndex={0}>{logs.shown.join("\n")}</pre>}
        </Section>
      ) : null}
      {view.status === "completed" && view.hasResult ? (
        <Section title="Result" note={isCutResult(view.result) && view.truncated ? "The result was over 256 KiB. This is the first 64 KiB, as text." : null}>
          <pre className="co scr" tabIndex={0}>{resultText(view)}</pre>
        </Section>
      ) : null}
      {error ? (
        <Section error title={<>Error · <span className="mono">{error.kind}</span>{typeof error.line === "number" ? ` · line ${error.line}` : ""}</>}>
          <div className="co">{error.message}</div>
          {error.kind === "lost" ? (
            <div className="cm-banner">Resume replays the script with the recorded calls. Calls that finished do not run again. A call with an effect that was still running ends the replay.</div>
          ) : null}
        </Section>
      ) : null}
      <Footer view={view} now={now} inspect={inspect} />
    </div>
  );
}
