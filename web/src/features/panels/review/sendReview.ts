import type { Thread } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { useWorkspace } from "../../../product/state/workspace";
import { reviewMessage } from "../../../product/composer/reviewBatch";
import type { ReviewChip } from "../../../product/composer/composerContext";
import { api } from "../api";

/** Threads with a send in flight: a second click or key press while one is open does nothing. */
const sending = new Set<string>();

export const isSending = (threadId: string) => sending.has(threadId);

/**
 * Sends every draft as ONE message through the same calls the composer uses: a run when the thread is idle, the queue
 * when a run is going. The drafts leave the composer only after the service accepts the message; a failure throws and
 * keeps every one of them. Returns false when a send for this thread was already in flight.
 */
export async function sendReviewDrafts(thread: Thread, drafts: readonly ReviewChip[]): Promise<boolean> {
  if (drafts.length === 0 || sending.has(thread.id)) return false;
  sending.add(thread.id);
  try {
    const prompt = reviewMessage(drafts, thread.worktree?.branch ?? null);
    const busy = (useWorkspace.getState().threads[thread.id] ?? thread).active_run !== null;
    const run = busy ? null : await api.startRun(thread.id, prompt, []);
    if (busy) await api.enqueue(thread.id, prompt, []);
    useComposerChips.getState().consume(thread.id, drafts);
    if (run) useThreadLogs.getState().addPrompt(thread.id, { runId: run.id, text: prompt });
    void useWorkspace.getState().refreshThread(thread.id);
    return true;
  } finally {
    sending.delete(thread.id);
  }
}
