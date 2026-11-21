import { api } from "../../../../api/client";
import type { Thread } from "../../../../api/types";

/** What Undo takes back: a run that just started, or a message that was queued behind a running one. */
export type Sent = { kind: "run"; id: string } | { kind: "queued"; id: string };

/**
 * The normal thread send, with no extra route: a message starts a run, or queues behind the run that is going.
 * One request. The caller holds the busy guard so a second Enter cannot send it again.
 */
export async function sendPrompt(thread: Pick<Thread, "id" | "active_run">, prompt: string): Promise<Sent> {
  if (thread.active_run) {
    const queued = await api.enqueue(thread.id, prompt, []);
    return { kind: "queued", id: queued.id };
  }
  const run = await api.startRun(thread.id, prompt);
  return { kind: "run", id: run.id };
}

/** Takes a sent prompt back: withdraws a queued message, or stops the run (which may already have begun). */
export async function takeBack(sent: Sent): Promise<void> {
  if (sent.kind === "queued") await api.removeQueued(sent.id);
  else await api.cancelRun(sent.id);
}
