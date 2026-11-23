import { useWorkspace } from "../../product/state/workspace";
import { ApiFailure, request } from "../../api/client";
import type { Thread } from "../../api/types";
import type { ThreadFamily } from "../../generated/ThreadFamily";
import { useAsync } from "../../lib/useAsync";

const q = encodeURIComponent;

/** GET /api/threads/{id}/family: the whole family as one tree, from any member. The route is merged. */
export async function readFamily(threadId: string, signal?: AbortSignal): Promise<ThreadFamily> {
  const family = await request<ThreadFamily>("GET", `/api/threads/${q(threadId)}/family`, undefined, signal);
  // A reply that is not a family is a failed read, not a family of none.
  if (!family || !Array.isArray(family.threads)) throw new ApiFailure("unexpected", "The family could not be read.", 0);
  return family;
}

export type FamilyRead =
  | { status: "loading"; family: null; reload(): void }
  | { status: "error"; family: null; reload(): void }
  | { status: "ready"; family: ThreadFamily; reload(): void };

/**
 * The thread's family. The read is made again when the app learns of a thread that has a parent, so a fork or a side
 * chat made here shows at once; the last answer stays on screen while it does.
 */
export function useFamily(threadId: string): FamilyRead {
  const stamp = useWorkspace((state) => Object.values(state.threads).filter((thread) => thread.lifecycle?.parent_thread_id).length);
  const read = useAsync(`family:${threadId}:${stamp}`, (signal) => readFamily(threadId, signal), { keep: true });
  if (read.status === "ready") return { status: "ready", family: read.data, reload: read.reload };
  // A failed re-read of a family already shown keeps the family.
  if (read.status === "error" && read.data) return { status: "ready", family: read.data, reload: read.reload };
  return { status: read.status === "error" ? "error" : "loading", family: null, reload: read.reload };
}

/** True when the app already knows the thread has a parent or a child, before the family is read. */
export function hasKnownRelatives(thread: Thread, threads: Readonly<Record<string, Thread>>): boolean {
  if (thread.lifecycle?.parent_thread_id) return true;
  return Object.values(threads).some((other) => other.lifecycle?.parent_thread_id === thread.id);
}
