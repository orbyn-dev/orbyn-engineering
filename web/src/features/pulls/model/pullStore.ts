import { create } from "zustand";
import { pullsApi } from "../api";
import type { PullRequest } from "../../../api/types";
import { checksOf } from "../../../product/pulls/labels";

/** What the chip and every list of a thread's pull requests share, so a change in one shows in both. */
export interface ThreadPullsEntry {
  /** In pull request number order, ascending; null until the first load answers. */
  pulls: PullRequest[] | null;
  /** Numbers the person is watching. */
  watched: number[];
  /** `mergeable`, `conflicting` or `unknown` per open pull request that could be ready to merge. */
  mergeable: Record<number, string | null>;
  /** When the list was last read, in ms. */
  loadedAt: number | null;
  /** A read or a refresh is in flight. */
  busy: boolean;
  /** The last refresh failed; the list is what was loaded before. */
  failed: boolean;
}

const empty: ThreadPullsEntry = { pulls: null, watched: [], mergeable: {}, loadedAt: null, busy: false, failed: false };

interface ThreadPullsStore {
  byThread: Record<string, ThreadPullsEntry>;
  /** Reads the list in the background. A list already known stays on screen. */
  load(thread: { id: string; project_id: string }): Promise<void>;
  /** Asks the forge again, which is a refresh the person started. */
  refresh(thread: { id: string; project_id: string }): Promise<void>;
  setWatch(threadId: string, number: number, on: boolean): Promise<void>;
  forget(threadId: string): void;
  /** A consumer shows this thread's list; the entry is dropped only when the last one lets go. */
  retain(threadId: string): () => void;
}

const byNumber = (pulls: readonly PullRequest[]) => [...pulls].sort((a, b) => a.number - b.number);

/** Open, not a draft, and its checks pass: the only ones worth asking whether they can merge. */
const couldBeReady = (pull: PullRequest) => pull.state === "open" && !pull.draft && checksOf(pull.checks_state) === "pass";

/** Most pull requests asked about at once: the rest keep the label they would have without it. */
const MAX_MERGEABLE_READS = 8;

export const useThreadPulls = create<ThreadPullsStore>((set, get) => {
  // The newest read per thread wins; an older one that lands late is ignored.
  const newest = new Map<string, number>();
  const pending = new Map<string, number>();
  const holders = new Map<string, number>();
  let counter = 0;
  const patch = (id: string, change: Partial<ThreadPullsEntry>) =>
    set((state) => ({ byThread: { ...state.byThread, [id]: { ...(state.byThread[id] ?? empty), ...change } } }));

  async function mergeables(thread: { id: string; project_id: string }, pulls: PullRequest[], known: Record<number, string | null>, fresh: boolean) {
    const wanted = pulls.filter(couldBeReady);
    const asked = wanted.filter((pull) => fresh || !(pull.number in known)).slice(0, MAX_MERGEABLE_READS);
    const entries = await Promise.all(
      asked.map((pull) =>
        pullsApi.detail(thread.project_id, pull.number).then(
          (detail) => [pull.number, detail.mergeable] as const,
          () => [pull.number, null] as const,
        ),
      ),
    );
    return { ...Object.fromEntries(wanted.filter((pull) => pull.number in known).map((pull) => [pull.number, known[pull.number] ?? null])), ...Object.fromEntries(entries) };
  }

  async function read(thread: { id: string; project_id: string }, work: Promise<PullRequest[]>, fresh: boolean) {
    const mine = ++counter;
    newest.set(thread.id, mine);
    pending.set(thread.id, (pending.get(thread.id) ?? 0) + 1);
    patch(thread.id, { busy: true });
    const current = () => newest.get(thread.id) === mine;
    try {
      // The list matters more than the Watch switches: a failed read of those leaves them off.
      const [raw, watches] = await Promise.all([work, pullsApi.watches(thread.id).catch(() => [])]);
      if (!current()) return;
      const pulls = byNumber(raw);
      patch(thread.id, { pulls, watched: watches.map((watch) => watch.number), loadedAt: Date.now(), failed: false });
      const mergeable = await mergeables(thread, pulls, get().byThread[thread.id]?.mergeable ?? {}, fresh);
      if (current()) patch(thread.id, { mergeable });
    } catch {
      if (current()) patch(thread.id, { failed: true });
    } finally {
      const left = (pending.get(thread.id) ?? 1) - 1;
      if (left <= 0) {
        pending.delete(thread.id);
        // A thread that was forgotten meanwhile stays forgotten.
        if (newest.has(thread.id)) patch(thread.id, { busy: false });
      } else pending.set(thread.id, left);
    }
  }

  return {
    byThread: {},
    load: (thread) => read(thread, pullsApi.linked(thread.id), false),
    refresh: (thread) => read(thread, pullsApi.refreshLinked(thread.id), true),
    async setWatch(threadId, number, on) {
      const before = get().byThread[threadId]?.watched ?? [];
      // The switch changes at once and the row keeps its place; a failure puts it back.
      patch(threadId, { watched: on ? [...before, number] : before.filter((entry) => entry !== number) });
      try {
        await (on ? pullsApi.watch(threadId, number) : pullsApi.unwatch(threadId, number));
      } catch (failure) {
        patch(threadId, { watched: before });
        throw failure;
      }
    },
    retain(threadId) {
      holders.set(threadId, (holders.get(threadId) ?? 0) + 1);
      return () => {
        holders.set(threadId, (holders.get(threadId) ?? 1) - 1);
        // A swap of one consumer for another unmounts before it mounts: look again once it has.
        setTimeout(() => {
          if ((holders.get(threadId) ?? 0) <= 0) {
            holders.delete(threadId);
            get().forget(threadId);
          }
        }, 0);
      };
    },
    forget(threadId) {
      newest.delete(threadId);
      pending.delete(threadId);
      set((state) => {
        const { [threadId]: _gone, ...rest } = state.byThread;
        return { byThread: rest };
      });
    },
  };
});

/** The entry a consumer renders for one thread, including an empty one before the first read. */
export function useThreadPullEntry(threadId: string): ThreadPullsEntry {
  return useThreadPulls((state) => state.byThread[threadId] ?? empty);
}
