import { Archive, GitFork, Loader2 } from "lucide-react";
import { useMemo } from "react";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { toFailure } from "../../lib/useAsync";
import { navigate } from "../../state/route";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { Button } from "../../ui/controls";
import { unarchiveThread } from "../workspace/threadActions";
import { usageShare } from "./ComposerExtras";
import { useThreadLogs } from "./threadLogs";

/** A run whose prompt is the compact command, while it runs: the thread is being compacted. */
export function useCompacting(thread: Thread): boolean {
  const items = useThreadLogs((state) => state.logs[thread.id]?.items);
  return useMemo(() => {
    if (!thread.active_run || !items) return false;
    const prompt = items.findLast((item) => item.kind === "message" && item.role === "user");
    return prompt?.kind === "message" && prompt.text.trim() === "/compact";
  }, [items, thread.active_run]);
}

const tokens = (count: number) => (count >= 1000 ? `${Math.round(count / 1000)}k` : String(count));

/** The compact step: a spinner and how large the thread is now. */
export function CompactingStep({ thread }: { thread: Thread }) {
  const compacting = useCompacting(thread);
  const usage = useThreadLogs((state) => state.logs[thread.id]?.usage);
  const share = usageShare(usage);
  if (!compacting) return null;
  return (
    <div className="compacting" role="status">
      <p className="compacting-step">
        <Loader2 size={14} className="spin" aria-hidden />
        {share ? `Compacting · ${tokens(share.used)} tokens` : "Compacting"}
      </p>
      <p className="compacting-note">Files, the plan and your messages are kept. Older tool output is summarised.</p>
    </div>
  );
}

/** An archived thread is read-only: unarchive it, or fork from its end. */
export function ArchivedBanner({ thread }: { thread: Thread }) {
  const pushToast = useUi((state) => state.pushToast);
  const putThread = useWorkspace((state) => state.putThread);
  const lastRun = useThreadLogs((state) => state.logs[thread.id]?.items.findLast((item) => item.runId !== null)?.runId ?? null);
  const fork = async () => {
    if (!lastRun) return;
    try {
      const child = await api.forkThread(thread.id, lastRun);
      putThread(child);
      navigate({ name: "thread", id: child.id });
    } catch (failure) {
      pushToast(`Couldn't fork. ${toFailure(failure).message}`);
    }
  };
  return (
    <div className="archived-banner" role="status">
      <Archive size={16} aria-hidden />
      <span>This thread is archived and read-only.</span>
      <span className="archived-actions">
        {lastRun ? (
          <Button small tone="quiet" icon={<GitFork size={14} aria-hidden />} onClick={() => void fork()}>
            Fork to new thread
          </Button>
        ) : null}
        <Button small tone="primary" disabled={thread.project_removed === true} onClick={() => void unarchiveThread(thread)}>
          Unarchive
        </Button>
      </span>
    </div>
  );
}
