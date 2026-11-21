import { RefreshCw } from "lucide-react";
import type { KeyboardEvent } from "react";
import type { Thread } from "../../../api/types";
import type { PullChangeBreakdown } from "../../../generated/PullChangeBreakdown";
import { compactAge } from "../../../lib/format";
import { useWaited } from "../../../lib/motion";
import { useAsync, toFailure } from "../../../lib/useAsync";
import { runStateLabels } from "../../../product/runs/stateLabels";
import { navigate } from "../../../product/state/route";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { pullChanged } from "../model/pullTopics";
import { pullsApi } from "../api";

type Kind = "added" | "modified" | "deleted" | "renamed";
const KINDS: { kind: Kind; label: string }[] = [
  { kind: "added", label: "Added" },
  { kind: "modified", label: "Modified" },
  { kind: "deleted", label: "Deleted" },
  { kind: "renamed", label: "Renamed" },
];

/** The bar's segments: each kind that has files, sized by its share of the four kinds. None when no file has a kind. */
export function breakdownBars(counts: PullChangeBreakdown): { kind: Kind; share: number }[] {
  const total = KINDS.reduce((sum, { kind }) => sum + Number(counts[kind]), 0);
  if (total === 0) return [];
  return KINDS.filter(({ kind }) => Number(counts[kind]) > 0).map(({ kind }) => ({ kind, share: (Number(counts[kind]) / total) * 100 }));
}

function Skeleton({ label }: { label: string }) {
  return (
    <div className="pull-linked-skel" aria-busy="true" aria-label={label}>
      <span /> <span />
    </div>
  );
}

function Failed({ text, detail, onRetry }: { text: string; detail?: string; onRetry(): void }) {
  return (
    <div className="pull-linked-failed" role="alert">
      <span className="muted">{text}{detail ? ` ${detail}` : ""}</span>
      <Button small icon={<RefreshCw size={13} aria-hidden />} onClick={onRetry}>
        Retry
      </Button>
    </div>
  );
}

/** The files by kind and the lines, as a bar with the numbers under it: never colour alone. */
export function ChangeBreakdownCard({ projectId, number }: { projectId: string; number: number }) {
  const read = useAsync(`pull-breakdown:${projectId}:${number}`, (signal) => pullsApi.breakdown(projectId, number, signal), { keep: true, refreshOn: pullChanged(projectId, number) });
  const waited = useWaited(read.status === "loading");
  const data = read.data;
  return (
    <div className="pull-card pull-breakdown">
      <b>Change breakdown</b>
      {read.status === "error" && !data ? (
        <Failed text="Can’t load the breakdown." detail={toFailure(read.error).message} onRetry={read.reload} />
      ) : data === null ? (
        waited ? <Skeleton label="Loading the breakdown" /> : null
      ) : (
        <>
          <div className="pull-bar" aria-hidden>
            {breakdownBars(data).map((bar) => (
              <i key={bar.kind} data-kind={bar.kind} style={{ width: `${bar.share}%` }} />
            ))}
          </div>
          <div className="pull-breakdown-kinds">
            {KINDS.map(({ kind, label }) => (
              <div key={kind}>
                <b>{Number(data[kind])}</b>
                <span data-kind={kind}>{label}</span>
              </div>
            ))}
          </div>
          <div className="pull-kv">
            <span>{Number(data.files)} {Number(data.files) === 1 ? "file" : "files"}</span>
            <b>
              <span className="pull-num">
                +{Number(data.additions)} <span className="pull-num-del">−{Number(data.deletions)}</span>
              </span>{" "}
              lines
            </b>
          </div>
        </>
      )}
    </div>
  );
}

/** Where a linked thread stands: working, waiting for the person, failed, or how long since it moved. */
function stand(thread: Thread): { word: string; tone: "run" | "warn" | "fail" | "age" } {
  const state = thread.active_run?.state ?? thread.last_run_state;
  if (state === "running" || state === "starting" || state === "admitted") return { word: runStateLabels[state], tone: "run" };
  if (state === "awaiting_approval") return { word: runStateLabels[state], tone: "warn" };
  if (state === "failed") return { word: runStateLabels[state], tone: "fail" };
  return { word: compactAge(thread.updated_at), tone: "age" };
}

/** Every thread the service links to this pull request; a row opens the thread. Titles are the people's text. */
export function LinkedThreadsCard({ projectId, number }: { projectId: string; number: number }) {
  const read = useAsync(`pull-linked-threads:${projectId}:${number}`, (signal) => pullsApi.linkedThreads(projectId, number, signal), { keep: true, refreshOn: pullChanged(projectId, number) });
  const engines = useWorkspace((state) => state.engines.data);
  const waited = useWaited(read.status === "loading");
  const data = read.data;
  const open = (thread: Thread) => navigate({ name: "thread", id: thread.id });
  const key = (event: KeyboardEvent, thread: Thread) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    open(thread);
  };
  const title = data && data.length > 0 ? `Linked threads · ${data.length}` : "Linked threads";
  return (
    <div className="pull-card pull-linked">
      <b>{title}</b>
      {read.status === "error" && !data ? (
        <Failed text="Can’t load the linked threads. Nothing is changed." onRetry={read.reload} />
      ) : data === null ? (
        waited ? <Skeleton label="Loading linked threads" /> : null
      ) : data.length === 0 ? (
        <span className="muted">No thread is linked to this pull request.</span>
      ) : (
        data.map((thread) => {
          const where = stand(thread);
          const engine = engines?.find((entry) => entry.id === thread.engine)?.name ?? thread.engine;
          return (
            <div key={thread.id} className="pull-linked-row" role="link" tabIndex={0} onClick={() => open(thread)} onKeyDown={(event) => key(event, thread)}>
              <span className="pull-linked-top">
                <b>{thread.title}</b>
                <span className="pull-linked-state" data-tone={where.tone}>{where.word}</span>
              </span>
              <span className="muted">
                {engine}
                {thread.worktree?.branch ? <> {"·"} <span className="pull-mono">{thread.worktree.branch}</span></> : null}
              </span>
            </div>
          );
        })
      )}
    </div>
  );
}
