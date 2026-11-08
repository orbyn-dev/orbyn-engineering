/**
 * The last good answer for a few screen reads, kept in memory so a revisit can draw
 * it at once while the read runs again. A read opts in with `useAsync(..., { keep: true })`
 * and its key is its resource identity. Nothing is written to disk. Entries expire after
 * ten minutes, are swept on every write, and the whole store is capped by an
 * approximate size so a large answer cannot crowd the rest out.
 */
const KEEP_MS = 10 * 60_000;
const MAX_ENTRIES = 200;
const MAX_TOTAL = 4_000_000;
const MAX_ONE = 1_000_000;

interface Kept {
  at: number;
  data: unknown;
  size: number;
}

const kept = new Map<string, Kept>();

/** Characters or bytes, roughly; an answer that cannot be measured is not kept. */
function approximateSize(data: unknown): number | null {
  if (typeof data === "string") return data.length;
  if (data instanceof ArrayBuffer) return data.byteLength;
  if (ArrayBuffer.isView(data)) return data.byteLength;
  try {
    return JSON.stringify(data)?.length ?? 0;
  } catch {
    return null;
  }
}

function sweep(now: number) {
  let total = 0;
  for (const [key, entry] of kept) {
    if (now - entry.at > KEEP_MS) kept.delete(key);
    else total += entry.size;
  }
  for (const [key, entry] of kept) {
    if (kept.size <= MAX_ENTRIES && total <= MAX_TOTAL) break;
    kept.delete(key);
    total -= entry.size;
  }
}

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
  const now = Date.now();
  const size = approximateSize(data);
  kept.delete(key);
  if (size !== null && size <= MAX_ONE) kept.set(key, { at: now, data, size });
  sweep(now);
}

export function forgetKept(prefix = "") {
  for (const key of [...kept.keys()]) if (key.startsWith(prefix)) kept.delete(key);
}
