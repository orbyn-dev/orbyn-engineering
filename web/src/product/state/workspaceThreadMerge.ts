import type { Thread } from "../../api/types";

/** Deep equality over the plain JSON the API answers with: an unchanged poll compares equal at every key. */
export function sameData(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  for (const key of keys) if (!sameData(left[key], right[key])) return false;
  return true;
}

/** Merges polled threads, keeping the objects whose content did not change, so their rows keep their props. */
export function reuseThreads(base: Record<string, Thread>, list: readonly Thread[]) {
  let next: Record<string, Thread> | null = null;
  for (const thread of list) {
    const known = (next ?? base)[thread.id];
    if (known !== undefined && sameData(known, thread)) continue;
    next ??= { ...base };
    next[thread.id] = thread;
  }
  return { changed: next !== null, threads: next ?? base };
}

