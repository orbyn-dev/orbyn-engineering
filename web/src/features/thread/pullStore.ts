import { create } from "zustand";
import { pullsApi } from "../../api/pulls";
import type { PullRequest } from "../../api/types";
import { checksOf } from "../pulls/pullModel";

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
}

const byNumber = (pulls: readonly PullRequest[]) => [...pulls].sort((a, b) => a.number - b.number);

/** Open, not a draft, and its checks pass: the only ones worth asking whether they can merge. */
const couldBeReady = (pull: PullRequest) => pull.state === "open" && !pull.draft && checksOf(pull.checks_state) === "pass";

export const useThreadPulls = create<ThreadPullsStore>((set, get) => {
  const patch = (id: string, change: Partial<ThreadPullsEntry>) =>
    set((state) => ({ byThread: { ...state.byThread, [id]: { ...(state.byThread[id] ?? empty), ...change } } }));

  async function mergeables(thread: { id: string; project_id: string }, pulls: PullRequest[]) {
    const entries = await Promise.all(
      pulls.filter(couldBeReady).map((pull) =>
        pullsApi.detail(thread.project_id, pull.number).then(
          (detail) => [pull.number, detail.mergeable] as const,
          () => [pull.number, null] as const,
        ),
      ),
    );
    return Object.fromEntries(entries);
  }

  async function read(thread: { id: string; project_id: string }, work: Promise<PullRequest[]>) {
    patch(thread.id, { busy: true });
    try {
      const [raw, watches] = await Promise.all([work, pullsApi.watches(thread.id)]);
      const pulls = byNumber(raw);
      patch(thread.id, { pulls, watched: watches.map((watch) => watch.number), loadedAt: Date.now(), failed: false });
      patch(thread.id, { mergeable: await mergeables(thread, pulls), busy: false });
    } catch {
      patch(thread.id, { busy: false, failed: true });
    }
  }

  return {
    byThread: {},
    load: (thread) => read(thread, pullsApi.linked(thread.id)),
    refresh: (thread) => read(thread, pullsApi.refreshLinked(thread.id)),
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
    forget: (threadId) =>
      set((state) => {
        const { [threadId]: _gone, ...rest } = state.byThread;
        return { byThread: rest };
      }),
  };
});

export const entryOf = (state: ThreadPullsStore, threadId: string): ThreadPullsEntry => state.byThread[threadId] ?? empty;

export type DotTone = "blue" | "red" | "ok" | "mute";

/** Precedence Failing, then Running, then Passing, as the Pull requests list reads checks. */
export function checksTone(pull: PullRequest): "fail" | "running" | "pass" | "skipped" | "none" | "unknown" {
  const counts = pull.checks;
  if (counts) {
    if (counts.failing > 0) return "fail";
    if (counts.running > 0) return "running";
    if (counts.passing > 0) return "pass";
    return counts.total > 0 ? "skipped" : "none";
  }
  const state = checksOf(pull.checks_state);
  return state === "fail" ? "fail" : state === "pending" ? "running" : state === "pass" ? "pass" : "none";
}

/** The sentence under a row: "4 of 4 checks passed". Null when the forge gave a state but no counts. */
export function checksSummary(pull: PullRequest): { tone: ReturnType<typeof checksTone>; text: string } | null {
  const tone = checksTone(pull);
  const counts = pull.checks;
  if (tone === "none") return { tone, text: "No checks" };
  if (!counts) return null;
  if (tone === "fail") return { tone, text: `${counts.failing} of ${counts.total} checks failed` };
  if (tone === "running") return { tone, text: `${counts.total - counts.running} of ${counts.total} checks done` };
  if (tone === "skipped") return { tone, text: "No checks ran" };
  return { tone, text: `${counts.passing} of ${counts.total} checks passed` };
}

/** Open, not a draft, checks passing and mergeable: the Pull requests list's own rule. */
export const readyToMerge = (pull: PullRequest, mergeable: string | null | undefined) =>
  pull.state === "open" && !pull.draft && checksOf(pull.checks_state) === "pass" && mergeable === "mergeable";

/** The state word the row leads with: Open, Merged or Closed. A draft leads with its own label instead. */
export const stateWord = (pull: PullRequest): "Open" | "Merged" | "Closed" | null =>
  pull.draft ? null : pull.state === "merged" ? "Merged" : pull.state === "closed" ? "Closed" : "Open";

/** A dot for the chip: blue while checks run, red when one failed, quiet otherwise. */
export function chipDot(pulls: readonly PullRequest[]): DotTone {
  const tones = pulls.map(checksTone);
  if (tones.includes("fail")) return "red";
  if (tones.includes("running")) return "blue";
  return tones.some((tone) => tone === "pass") ? "ok" : "mute";
}

export const chipLabel = (pulls: readonly PullRequest[]) => (pulls.length === 1 ? `#${pulls[0]!.number}` : `PRs ${pulls.length}`);

/** "2 min ago", from the time the list was read. */
export function agoWords(at: number, now: number): string {
  const minutes = Math.max(0, Math.floor((now - at) / 60_000));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} hr ago` : `${Math.floor(hours / 24)} d ago`;
}

/** The forge's name from a pull request's address, for the line that says it did not answer. */
export function forgeName(url: string | undefined): string {
  const host = (() => {
    try {
      return new URL(url ?? "").hostname;
    } catch {
      return "";
    }
  })();
  if (host.includes("github")) return "GitHub";
  if (host.includes("gitlab")) return "GitLab";
  if (host.includes("dev.azure") || host.includes("visualstudio")) return "Azure DevOps";
  if (host.includes("forgejo") || host.includes("codeberg")) return "Forgejo";
  return "GitHub";
}

export const WATCH_TIP =
  "Tells you when a check fails, all checks pass, the branch conflicts with its base, or there are new comments or reviews. Yours are skipped.";
