import { useNow } from "../../lib/useNow";
import { CircleAlert, Clock, Gauge, RotateCcw, X } from "lucide-react";
import { useRef, useState } from "react";
import { api } from "./api";
import type { Thread, UpdateThread } from "../../api/types";
import { waitLeft } from "../../lib/format";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Button, IconButton } from "../../ui/controls";

const dismissed = new Set<string>();

const CONTINUE = "Continue where you left off.";

const resetLabel = (at: string) =>
  new Date(at).toLocaleString(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" });

/** An engine stopped on a limit: its words, when it comes back, and what to do until then. */
export function LimitBanner({ thread }: { thread: Thread }) {
  const refreshThread = useWorkspace((state) => state.refreshThread);
  const putThread = useWorkspace((state) => state.putThread);
  const pushToast = useUi((state) => state.pushToast);
  const limit = thread.lifecycle?.limit;
  const resetsAt = limit?.resets_at ? Date.parse(limit.resets_at) : Number.NaN;
  // The clock ticks only while a wait is still ahead, and stops once it is over.
  const now = useNow(!Number.isNaN(resetsAt) && Date.now() < resetsAt + 1000);
  if (!limit || thread.active_run) return null;
  const failed = (error: unknown) => pushToast(`Couldn't do that. ${toFailure(error).message}`);
  const patch = (body: UpdateThread) => api.patchThread(thread.id, body).then(putThread, failed);
  const future = !Number.isNaN(resetsAt) && resetsAt > now;
  const over = !Number.isNaN(resetsAt) && !future;
  const snoozedUntilReset = future && thread.lifecycle?.snoozed_until != null &&
    Date.parse(thread.lifecycle.snoozed_until) === resetsAt;
  const minutes = Math.ceil((resetsAt - now) / 60_000);
  return (
    <div className="thread-error thread-limit" role="group" aria-label="Limited">
      <Gauge size={14} aria-hidden />
      {/* Only text that does not change each second is announced; the countdown itself is not. */}
      <span role="status">
        <span className="limit-label">Rate limited</span> {limit.message}
        {limit.resets_at ? ` Resets ${resetLabel(limit.resets_at)}.` : ""}
        {over ? " The wait is over." : ""}
        {future ? <span className="sr-only">{` Try again in about ${minutes === 1 ? "a minute" : `${minutes} minutes`}.`}</span> : null}
      </span>
      {future ? <LimitCountdown limit={limit} now={now} /> : null}
      <Button
        small
        tone="quiet"
        icon={<RotateCcw size={12} aria-hidden />}
        onClick={() => {
          api.startRun(thread.id, CONTINUE).then(() => void refreshThread(thread.id), failed);
        }}
      >
        Resume
      </Button>
      {future ? (
        <Button small tone="quiet" onClick={() => void patch({ resume_at_reset: limit.resume_at === null })}>
          {limit.resume_at ? "Cancel auto-resume" : "Resume at reset"}
        </Button>
      ) : null}
      {future ? (
        <Button
          small
          tone="quiet"
          icon={<Clock size={12} aria-hidden />}
          onClick={() => void patch(snoozedUntilReset ? { wake: true } : { snoozed_until: limit.resets_at ?? undefined })}
        >
          {snoozedUntilReset ? "Wake now" : "Snooze until reset"}
        </Button>
      ) : null}
    </div>
  );
}

/** The latest failure, with a way to send the same prompt again. Dismissal lasts the session. */
export function ThreadErrorBanner({ thread }: { thread: Thread }) {
  const items = useThreadLogs((state) => state.logs[thread.id]?.items);
  const [, redraw] = useState(0);
  const refreshThread = useWorkspace((state) => state.refreshThread);
  const pushToast = useUi((state) => state.pushToast);
  if (thread.active_run || thread.last_run_state !== "failed" || thread.lifecycle?.limit || !items) return null;
  const failure = items.findLast((item) => item.kind === "notice" && item.tone === "error");
  if (!failure || failure.kind !== "notice" || dismissed.has(failure.key)) return null;
  const prompt = items.findLast((item) => item.kind === "message" && item.role === "user");
  if (failure.code === "workspace_preparation_failed" || failure.code === "workspace_preparation_unavailable") return null;
  const retry = () => {
    if (prompt?.kind !== "message") return;
    api.startRun(thread.id, prompt.text).then(
      () => void refreshThread(thread.id),
      (error: unknown) => pushToast(`Couldn't retry. ${toFailure(error).message}`),
    );
  };
  return (
    <div className="thread-error">
      <CircleAlert size={14} aria-hidden />
      <span>{failure.text}</span>
      {prompt ? (
        <Button small tone="quiet" icon={<RotateCcw size={12} aria-hidden />} onClick={retry}>
          Retry
        </Button>
      ) : null}
      <IconButton
        label="Dismiss error"
        onClick={() => {
          dismissed.add(failure.key);
          redraw((count) => count + 1);
        }}
      >
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}

/** A preparation failure keeps its recovery action with the failed run. */
export function PreparationRetryButton({ threadId, runId }: { threadId: string; runId: string }) {
  const thread = useWorkspace((state) => state.threads[threadId]);
  const refreshThread = useWorkspace((state) => state.refreshThread);
  const pushToast = useUi((state) => state.pushToast);
  const [retrying, setRetrying] = useState(false);
  const pending = useRef(false);
  const candidate = useAsync(
    `preparation:${threadId}:${thread?.active_run?.id ?? "idle"}:${thread?.last_run_state}:${thread?.updated_at}`,
    (signal) => api.preparationRetry(threadId, signal),
  );
  if (thread?.active_run || candidate.data?.run_id !== runId) return null;
  const retry = () => {
    if (pending.current) return;
    pending.current = true;
    setRetrying(true);
    api.retryPreparation(threadId, runId).then(
      () => refreshThread(threadId),
      (error: unknown) => pushToast(`Couldn't retry. ${toFailure(error).message}`),
    ).finally(() => {
      pending.current = false;
      setRetrying(false);
    });
  };
  return <Button small tone="quiet" disabled={retrying} icon={<RotateCcw size={12} aria-hidden />} onClick={retry}>Retry</Button>;
}

/** "Try again in 2m 14s", counting down, with a bar for how much of the wait has passed. Not a live region. */
function LimitCountdown({ limit, now }: { limit: { stopped_at: string; resets_at: string | null }; now: number }) {
  const end = limit.resets_at ? Date.parse(limit.resets_at) : Number.NaN;
  const start = Date.parse(limit.stopped_at);
  const share = Number.isNaN(start) || end <= start ? 0 : Math.min(Math.max((now - start) / (end - start), 0), 1);
  return (
    <span className="limit-wait" aria-hidden>
      Try again in <b className="limit-left">{waitLeft(end - now)}</b>.
      <span className="xbar" role="presentation">
        <i style={{ width: `${Math.round(share * 100)}%` }} />
      </span>
    </span>
  );
}
