import { api } from "../api";
import type { Thread, UpdateThread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { askCloseLead, type CloseOutcome } from "../../crew";
import { pullsApi } from "../api";
import { threadToken } from "../../../product/composer/skillMention";
import { navigate, parseRoute } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { copyText } from "../../../product/ui/copyText";
import { arrangeThreads, threadsFlat } from "./groups";
import { pushUndo } from "./threadUndo";

const openThreadId = () => {
  const route = parseRoute(window.location.hash);
  return route.name === "thread" ? route.id : null;
};

const toastFailure = (what: string, failure: unknown) =>
  useUi.getState().pushToast(`Couldn't ${what}. ${toFailure(failure).message}`);

const patch = async (thread: Thread, body: UpdateThread) => {
  const next = await api.patchThread(thread.id, body);
  useWorkspace.getState().putThread(next);
  return next;
};

/** Leaves a thread that has just gone to a shelf, for the next row in the list. */
function moveOff(ids: ReadonlySet<string>) {
  const open = openThreadId();
  if (!open || !ids.has(open)) return;
  const all = Object.values(useWorkspace.getState().threads);
  const rows = threadsFlat(arrangeThreads(all, Date.now()));
  const from = rows.findIndex((thread) => thread.id === open);
  const after = [...rows.slice(from + 1), ...rows.slice(0, Math.max(from, 0))];
  const next = after.find(
    (thread) => !ids.has(thread.id) && !thread.lifecycle?.settled_at && !thread.lifecycle?.snoozed_until,
  );
  navigate(next ? { name: "thread", id: next.id } : { name: "new", projectId: null });
}

/** What restores a thread after settling, which also ends its pin and snooze. */
function restoreSettled(thread: Thread): UpdateThread {
  const lifecycle = thread.lifecycle;
  return {
    settled: false,
    ...(lifecycle?.pinned_at ? { pinned: true } : {}),
    ...(lifecycle?.snoozed_until ? { snoozed_until: lifecycle.snoozed_until } : {}),
  };
}

async function each(
  threads: readonly Thread[],
  what: string,
  body: (thread: Thread) => UpdateThread,
  after?: (thread: Thread) => void,
) {
  await Promise.all(
    threads.map((thread) =>
      patch(thread, body(thread)).then(
        () => after?.(thread),
        (failure: unknown) => toastFailure(what, failure),
      ),
    ),
  );
}

interface AskedThread {
  thread: Thread;
  outcome: CloseOutcome;
}

/** One close question at a time, in the order the person's batch named the threads. */
async function askEach(threads: readonly Thread[], verb: "settle" | "archive"): Promise<AskedThread[]> {
  const asked: AskedThread[] = [];
  for (const thread of threads) asked.push({ thread, outcome: await askCloseLead(thread, verb) });
  return asked;
}

const isClosed = (outcome: CloseOutcome): outcome is Extract<CloseOutcome, { kind: "closed" }> => outcome.kind === "closed";

/** What a thread with live work leaves instead of Undo: the counts, and the first task that could not be stopped. */
function toastClosed(verb: "settle" | "archive", outcome: Extract<CloseOutcome, { kind: "closed" }>) {
  const word = verb === "settle" ? "settled" : "archived";
  const count = outcome.closed.length;
  const cancelled = `${count} ${count === 1 ? "task" : "tasks"} cancelled.`;
  if (outcome.failed.length === 0) {
    useUi.getState().pushToast(`Thread ${word}. ${cancelled}`, "done");
    return;
  }
  const firstFailed = outcome.failed[0] ?? "";
  const name = outcome.tasks.find((task) => task.id === firstFailed)?.name ?? (firstFailed || "a task");
  const stopped = `${outcome.failed.length} could not be stopped: ${name}. Its worker may still be running. Stop it again from the Crew board.`;
  useUi.getState().pushToast(
    `Thread ${word}. ${count === 0 ? "" : `${cancelled} `}${stopped}`,
    "failed",
    {
      label: "Open task",
      run: () => {
        if (firstFailed) navigate({ name: "crew", tab: "flow", task: firstFailed });
      },
    },
    { durationMs: 12000 },
  );
}

export async function settleThreads(threads: readonly Thread[]) {
  const moving = threads.filter((thread) => !thread.lifecycle?.settled_at);
  const accepted = (await askEach(moving, "settle")).filter((entry) => entry.outcome.kind !== "declined");
  if (accepted.length === 0) return;
  moveOff(new Set(accepted.map((entry) => entry.thread.id)));
  await each(
    accepted.map((entry) => entry.thread),
    "settle the thread",
    () => ({ settled: true }),
    (thread) => {
      const outcome = accepted.find((entry) => entry.thread.id === thread.id)?.outcome;
      if (outcome && isClosed(outcome)) {
        toastClosed("settle", outcome);
        return;
      }
      pushUndo("Settled", () => patch(thread, restoreSettled(thread)).catch((f: unknown) => toastFailure("undo", f)));
    },
  );
}

export const unsettleThreads = (threads: readonly Thread[]) =>
  each(threads, "un-settle the thread", () => ({ settled: false }));

export function snoozeThreads(threads: readonly Thread[], until: Date) {
  moveOff(new Set(threads.map((thread) => thread.id)));
  const snoozed_until = until.toISOString();
  return each(threads, "snooze the thread", () => ({ snoozed_until }), (thread) =>
    pushUndo("Snoozed", () => patch(thread, { wake: true }).catch((f: unknown) => toastFailure("undo", f))),
  );
}

export const dismissWoke = (thread: Thread) =>
  each([thread], "dismiss that", () => ({ dismiss_woke: true }));

export const wakeThreads = (threads: readonly Thread[]) =>
  each(threads, "wake the thread", () => ({ wake: true }));

export const pinThreads = (threads: readonly Thread[]) =>
  each(threads, "pin the thread", () => ({ pinned: true }));

const declined = (question: string) => !window.confirm(question);

export const unpinThreads = async (threads: readonly Thread[]) =>
  useUi.getState().confirmUnpin && declined("Unpin this thread?")
    ? undefined
    : each(threads, "unpin the thread", () => ({ pinned: false }), (thread) =>
    pushUndo("Unpinned", () => patch(thread, { pinned: true }).catch((f: unknown) => toastFailure("undo", f))),
  );

export async function archiveThreads(threads: readonly Thread[]) {
  const moving = threads.filter((thread) => !thread.lifecycle?.archived_at);
  const accepted = (await askEach(moving, "archive")).filter((entry) => entry.outcome.kind !== "declined");
  if (accepted.length === 0) return undefined;
  // Cancelling live work is the reason to archive; a thread with no live work still asks, as it did before.
  let keeping = accepted;
  if (useUi.getState().confirmArchive && accepted.some((entry) => entry.outcome.kind === "none")) {
    if (declined("Archive this thread?")) {
      // A declined question drops only the threads it was asked for; work already cancelled still archives.
      keeping = accepted.filter((entry) => entry.outcome.kind === "closed");
      if (keeping.length === 0) return undefined;
    }
  }
  moveOff(new Set(keeping.map((entry) => entry.thread.id)));
  await each(
    keeping.map((entry) => entry.thread),
    "archive the thread",
    () => ({ archived: true }),
    (thread) => {
      const outcome = keeping.find((entry) => entry.thread.id === thread.id)?.outcome;
      if (outcome && isClosed(outcome)) {
        toastClosed("archive", outcome);
        return;
      }
      pushUndo("Archived", () => patch(thread, { archived: false }).catch((f: unknown) => toastFailure("undo", f)));
    },
  );
}

export const unarchiveThread = (thread: Thread) =>
  each([thread], "unarchive the thread", () => ({ archived: false }));

export const setUnread = (threads: readonly Thread[], unread: boolean) =>
  each(threads, "update the thread", () => ({ unread }));

export const setAutoSettle = (thread: Thread, choice: "enabled" | "disabled") =>
  each([thread], "update the thread", () => ({ auto_settle: choice }));

export const renameThread = (thread: Thread, title: string) =>
  each([thread], "rename the thread", () => ({ title }));

export function regenerateTitles(threads: readonly Thread[]) {
  return Promise.all(
    threads.map((thread) =>
      api.regenerateTitle(thread.id).then(
        (next) => useWorkspace.getState().putThread(next),
        (failure: unknown) => toastFailure("regenerate the title", failure),
      ),
    ),
  );
}

/** Deletes one thread and only then leaves it: a refusal reaches the caller, and nothing moves. */
export async function deleteThreadNow(thread: Thread) {
  await api.deleteThread(thread.id);
  moveOff(new Set([thread.id]));
  useWorkspace.getState().dropThread(thread);
}

export async function deleteThreads(threads: readonly Thread[]) {
  moveOff(new Set(threads.map((thread) => thread.id)));
  await Promise.all(
    threads.map((thread) =>
      api.deleteThread(thread.id).then(
        () => useWorkspace.getState().dropThread(thread),
        (failure: unknown) => toastFailure("delete the thread", failure),
      ),
    ),
  );
}

/** Saves `ordered` for a section, showing it at once. Ids left out keep their place. */
export async function reorderThreads(ordered: readonly Thread[]) {
  const { putThread } = useWorkspace.getState();
  const all = Object.values(useWorkspace.getState().threads);
  const lowest = all.reduce((low, thread) => Math.min(low, thread.lifecycle?.position ?? 0), 0);
  ordered.forEach((thread, index) => {
    const current = useWorkspace.getState().threads[thread.id] ?? thread;
    if (current.lifecycle) {
      putThread({
        ...current,
        lifecycle: { ...current.lifecycle, position: lowest - ordered.length + index },
      });
    }
  });
  try {
    await api.reorderThreads(ordered.map((thread) => thread.id));
  } catch (failure) {
    toastFailure("save the order", failure);
    await useWorkspace.getState().refreshAll();
  }
}

/** The folder stays where it is and becomes a project, with every thread in it. */
export async function turnIntoProject(thread: Thread) {
  try {
    const project = await api.adoptScratch(thread.id);
    const { putProject, refreshThreads } = useWorkspace.getState();
    putProject(project);
    await refreshThreads(project.id);
  } catch (failure) {
    toastFailure("turn it into a project", failure);
  }
}

/** The thread's pull request if it has one, otherwise a link that pastes as a thread chip. */
export async function copyReference(thread: Thread) {
  const pulls = await pullsApi.linked(thread.id).catch(() => []);
  const first = pulls[0];
  if (first) await copyText(first.url, `Copied #${first.number}`);
  else await copyText(threadToken(thread.id, thread.title), "Copied thread reference");
}
