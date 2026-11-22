import type { PullConversation, PullRequest, PullThread } from "../../../api/types";
import { pullsApi } from "../../../api/pulls";
import { useAsync, type AsyncState } from "../../../lib/useAsync";

/** How many linked pull requests one thread's review tab reads at once. */
export const MAX_FORGE_PULLS = 5;

export interface ForgePull {
  pull: PullRequest;
  conversation: PullConversation;
}

export type ForgeRead = AsyncState<ForgePull[]> & { reload(): void };

export interface ForgeGroups {
  open: PullThread[];
  resolved: PullThread[];
}

/** Every review thread the forge answered with, split by whether the forge marks it resolved. */
export function forgeGroups(reads: readonly ForgePull[] | null | undefined): ForgeGroups {
  const threads = (reads ?? []).flatMap((read) => read.conversation.threads);
  return {
    open: threads.filter((thread) => !thread.resolved),
    resolved: threads.filter((thread) => thread.resolved),
  };
}

/** The pull requests to read: the open ones first, the forge's order kept, at most five. */
export function forgePulls(linked: readonly PullRequest[]): PullRequest[] {
  return [...linked]
    .sort((left, right) => Number(right.state === "open") - Number(left.state === "open"))
    .slice(0, MAX_FORGE_PULLS);
}

/**
 * The thread's linked pull requests and their forge review threads. One `useAsync` key covers the
 * whole chain, so a new thread drops the old answer and a slow earlier read can never replace a
 * newer one.
 */
export function useForgeComments(threadId: string, projectId: string): ForgeRead {
  return useAsync(`forge-comments:${threadId}:${projectId}`, async (signal) => {
    if (!projectId) return [];
    const linked = await pullsApi.linked(threadId, signal);
    const chosen = forgePulls(linked);
    return Promise.all(chosen.map(async (pull) => ({
      pull,
      conversation: await pullsApi.conversation(projectId, pull.number, signal),
    })));
  });
}
