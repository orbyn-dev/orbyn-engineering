import type { Thread } from "../../api/types";
import type { ThreadFamily } from "../../generated/ThreadFamily";
import { compactAge } from "../../lib/format";
import { runStateLabels } from "../../product/runs/stateLabels";

/** How a thread was made from another one. The service sends the word; any other word is not drawn. */
export type Relation = "fork" | "side" | "subagent";

const LABELS: Record<Relation, string> = { fork: "Fork", side: "Side chat", subagent: "Subagent" };

export const relationLabel = (word: string | null | undefined): string | null =>
  word === "fork" || word === "side" || word === "subagent" ? LABELS[word] : null;

export interface FamilyRow {
  thread: Thread;
  /** 0 for the root, and for a member whose parent the family does not hold. */
  depth: number;
  relation: Relation | null;
  root: boolean;
  /** The thread that is open. */
  current: boolean;
}

/** The family as the service sends it, root first and every parent before its children, each with its depth. */
export function familyRows(family: ThreadFamily, currentId: string): FamilyRow[] {
  const known = new Map(family.threads.map((thread) => [thread.id, thread]));
  const depthOf = (thread: Thread): number => {
    let depth = 0;
    const seen = new Set([thread.id]);
    for (let at = parentIdOf(thread); at !== null; ) {
      const parent = known.get(at);
      if (!parent || seen.has(parent.id)) break;
      seen.add(parent.id);
      depth += 1;
      at = parentIdOf(parent);
    }
    return depth;
  };
  return family.threads.map((thread) => {
    const word = thread.lifecycle?.relation;
    return {
      thread,
      depth: depthOf(thread),
      relation: word === "fork" || word === "side" || word === "subagent" ? word : null,
      root: thread.id === family.root_thread_id,
      current: thread.id === currentId,
    };
  });
}

const parentIdOf = (thread: Thread): string | null => thread.lifecycle?.parent_thread_id ?? null;

/** The other threads in the family: what the chip counts. */
export const otherCount = (family: ThreadFamily): number => Math.max(0, family.threads.length - 1);

/** "Fork of <parent>": only when the thread has a parent that is known and a relation the app names. */
export function parentLine(thread: Thread, threads: Readonly<Record<string, Thread>>): { relation: string; parent: Thread } | null {
  const parentId = parentIdOf(thread);
  const parent = parentId ? threads[parentId] : undefined;
  const relation = relationLabel(thread.lifecycle?.relation);
  return parent && relation ? { relation, parent } : null;
}

export type RowStatus = { kind: "running" | "waiting" | "failed" | "age"; label: string };

/** Where a thread stands: working, waiting for the person, failed, or how long since it last moved. */
export function rowStatus(thread: Thread, now: number): RowStatus {
  const state = thread.active_run?.state ?? thread.last_run_state;
  if (state === "running" || state === "starting" || state === "admitted") return { kind: "running", label: runStateLabels[state] };
  if (state === "awaiting_approval") return { kind: "waiting", label: runStateLabels.awaiting_approval };
  if (state === "failed") return { kind: "failed", label: runStateLabels.failed };
  return { kind: "age", label: compactAge(thread.updated_at, now) };
}
