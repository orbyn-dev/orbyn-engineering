import { useEffect, useState } from "react";
import "./codemode.css";
import { useAsync } from "../../lib/useAsync";
import type { ToolItem } from "../../product/timeline/types";
import { listCodeRuns } from "./api";
import { callWord, capText } from "./model";
import { CodeRunBlock } from "./RunBlock";
import type { CodeRunSummary } from "./types";

const POLL_MS = 2_000;
const HEAD_CHARS = 120;
const RUN_TOOL = "mcp__orbyn_engineering__orbyn_code_run";

const STATUS_WORDS = new Map<string, { label: string; tone: "run" | "wait" | "done" | "fail" | "unknown" }>(Object.entries({
  running: { label: "Running", tone: "run" },
  paused: { label: "Paused", tone: "wait" },
  completed: { label: "Completed", tone: "done" },
  error: { label: "Error", tone: "fail" },
}));

/** A status the list does not know is shown as Unknown, never as a success. */
const statusOf = (status: string) => STATUS_WORDS.get(status) ?? { label: "Unknown", tone: "unknown" as const };

/** `12:41:08`, 24-hour, with seconds, as the frame writes a run's times. */
const stamp = (iso: string | null): string => {
  const at = iso ? new Date(iso) : null;
  return at && !Number.isNaN(at.getTime()) ? at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }) : "";
};

function when(run: CodeRunSummary): string {
  const calls = callWord(Number.isInteger(run.calls) && run.calls >= 0 ? run.calls : 0);
  return run.ended_at ? `${stamp(run.started_at)} to ${stamp(run.ended_at)} · ${calls}` : `Started ${stamp(run.started_at)} · ${calls}`;
}

const head = (text: string): string => {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > HEAD_CHARS ? `${flat.slice(0, HEAD_CHARS)}…` : flat;
};

/** The same block as in the thread, for one run: the record is read by its id, and the reply carries only the status. */
function RunDetail({ run }: { run: CodeRunSummary }) {
  const status = run.status;
  const item: ToolItem = {
    kind: "tool", key: `runs-list:${run.run_id}`, callId: run.run_id, seq: 0, runId: "", ts: run.started_at, endedAt: run.ended_at ?? undefined,
    name: RUN_TOOL, args: JSON.stringify({ run_id: run.run_id }), output: "", result: JSON.stringify({ status, runId: run.run_id }), done: true, exitCode: null,
  };
  return <CodeRunBlock item={item} live={false} inspect={null} />;
}

/**
 * The runs of this thread, newest first: id, status, when, call count and the first 120 characters of the script. The list holds no
 * calls; opening a run shows the same block as in the thread. There is no Stop: a paused run ends when its pending approval is
 * declined. Where this opens is not settled, so it sits under the Tools tab for now. When the service does not offer Code Mode
 * (every route answers 404) or the thread has no runs, nothing is drawn.
 */
export function CodeRunsList({ threadId }: { threadId: string }) {
  // `null` (Code Mode not offered) and a reply that is not a list are no list: nothing is drawn.
  const read = useAsync(`code-runs:${threadId}`, async (signal) => (await listCodeRuns(threadId, signal)) ?? []);
  const [open, setOpen] = useState<string | null>(null);
  const runs = read.status === "ready" ? read.data : [];
  const moving = runs.some((run) => run.status === "running" || run.status === "paused");
  const { refreshQuietly } = read;
  useEffect(() => {
    if (!moving) return;
    const timer = setInterval(() => void refreshQuietly(), POLL_MS);
    return () => clearInterval(timer);
  }, [moving, refreshQuietly]);
  if (runs.length === 0) return null;
  return (
    <section className="cm-runs" aria-label="Code runs">
      <div className="cm-runs-h">Code runs</div>
      {runs.slice(0, 100).map((run, at) => {
        const word = statusOf(run.status);
        const opened = open === run.run_id;
        return (
          <div key={`${at}-${run.run_id}`} className="cm-run" data-status={word.tone}>
            <button type="button" className="cm-run-row" aria-expanded={opened} disabled={word.tone === "unknown"} onClick={() => setOpen(opened ? null : run.run_id)}>
              <span className="cm-run-top"><span className="cm-id">{capText(String(run.run_id), 64)}</span><span className={`cm-run-status ${word.tone}`}>{word.label}</span></span>
              <span className="cm-run-when">{when(run)}</span>
              {run.script_head ? <span className="cm-run-head mono">{head(run.script_head)}</span> : null}
            </button>
            {opened ? <RunDetail run={run} /> : null}
          </div>
        );
      })}
      <p className="cm-runs-foot">Open a run to see its calls. Runs are kept for 7 days.</p>
    </section>
  );
}
