/**
 * The last good answer for each screen read, kept in memory so a revisit can draw
 * it at once while the read runs again in the background. Keys are the resource
 * identities `useAsync` already uses. Nothing is written to disk.
 */
const KEEP_MS = 10 * 60_000;
const MAX_ENTRIES = 200;

interface Kept {
  at: number;
  data: unknown;
}

const kept = new Map<string, Kept>();

export function readKept<T>(key: string): T | undefined {
  const entry = kept.get(key);
  if (!entry) return undefined;
  if (Date.now() - entry.at > KEEP_MS) {
    kept.delete(key);
    return undefined;
  }
  return entry.data as T;
}

export function writeKept(key: string, data: unknown) {
  kept.delete(key);
  kept.set(key, { at: Date.now(), data });
  while (kept.size > MAX_ENTRIES) {
    const oldest = kept.keys().next().value;
    if (oldest === undefined) break;
    kept.delete(oldest);
  }
}

export function forgetKept(prefix = "") {
  for (const key of [...kept.keys()]) if (key.startsWith(prefix)) kept.delete(key);
}
