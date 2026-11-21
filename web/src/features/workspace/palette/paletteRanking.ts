/**
 * Ranking for the command palette: how well the text matches, then frecency (how often and how
 * recently the person picked a row). The book holds only row ids and timestamps, never a title
 * or a branch name, and every function takes the clock so a ranking is reproducible.
 */

export interface Rankable {
  key: string;
  group: string;
  label: string;
  search?: string;
}

/** Row id to the times it was picked, newest first. */
export type FrecencyBook = Record<string, number[]>;

export const FRECENCY_MAX_IDS = 200;
export const FRECENCY_MAX_STAMPS = 5;
const STORAGE_KEY = "orbyn.palette.frecency";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
/** Weight of one pick by its age; older picks count less. */
const WEIGHTS: ReadonlyArray<readonly [number, number]> = [
  [4 * HOUR, 100],
  [DAY, 80],
  [3 * DAY, 60],
  [7 * DAY, 40],
  [30 * DAY, 20],
];
const OLDEST_WEIGHT = 10;

export function frecencyScore(stamps: readonly number[], now: number): number {
  let total = 0;
  for (const stamp of stamps) {
    const age = now - stamp;
    if (age < 0) continue;
    total += WEIGHTS.find(([limit]) => age < limit)?.[1] ?? OLDEST_WEIGHT;
  }
  return total;
}

/** A new book with one more pick; the input is untouched. Keeps the newest stamps and the most recently used ids. */
export function recordUse(book: FrecencyBook, id: string, now: number): FrecencyBook {
  const next: FrecencyBook = { ...book, [id]: [now, ...(book[id] ?? [])].slice(0, FRECENCY_MAX_STAMPS) };
  const ids = Object.keys(next);
  if (ids.length <= FRECENCY_MAX_IDS) return next;
  const newest = (one: string) => next[one]?.[0] ?? 0;
  ids.sort((a, b) => newest(b) - newest(a));
  return Object.fromEntries(ids.slice(0, FRECENCY_MAX_IDS).map((one) => [one, next[one] ?? []]));
}

/** 3: a title starting with every word, 2: a title containing them, 1: only the extra words (project, branch). 0: no query. */
function matchTier(entry: Rankable, words: readonly string[]): number {
  if (words.length === 0) return 0;
  const label = entry.label.toLowerCase();
  if (words.every((word) => label.startsWith(word) || label.includes(` ${word}`))) return label.startsWith(words[0] ?? "") ? 3 : 2;
  return words.every((word) => label.includes(word)) ? 2 : 1;
}

/**
 * Orders rows inside each group (the group order stays as given): better text match first, then
 * higher frecency, then the given order. Frecency never lifts a worse text match above a better one.
 */
export function rankEntries<T extends Rankable>(entries: readonly T[], query: string, book: FrecencyBook, now: number): T[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const groups = new Map<string, Array<{ entry: T; at: number }>>();
  entries.forEach((entry, at) => {
    const list = groups.get(entry.group) ?? [];
    list.push({ entry, at });
    groups.set(entry.group, list);
  });
  const out: T[] = [];
  for (const list of groups.values()) {
    const scored = list.map((one) => ({ ...one, tier: matchTier(one.entry, words), frecency: frecencyScore(book[one.entry.key] ?? [], now) }));
    scored.sort((a, b) => b.tier - a.tier || b.frecency - a.frecency || a.at - b.at);
    out.push(...scored.map((one) => one.entry));
  }
  return out;
}

export interface BookStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function browserStorage(): BookStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** What is stored, kept only if it is an id with finite timestamps; anything else is dropped. */
export function loadFrecency(area: BookStorage | null = browserStorage()): FrecencyBook {
  try {
    const parsed: unknown = JSON.parse(area?.getItem(STORAGE_KEY) ?? "{}");
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const book: FrecencyBook = {};
    for (const [id, stamps] of Object.entries(parsed).slice(0, FRECENCY_MAX_IDS)) {
      if (!Array.isArray(stamps)) continue;
      const kept = stamps.filter((one): one is number => typeof one === "number" && Number.isFinite(one)).slice(0, FRECENCY_MAX_STAMPS);
      if (kept.length > 0) book[id] = kept;
    }
    return book;
  } catch {
    return {};
  }
}

export function saveFrecency(book: FrecencyBook, area: BookStorage | null = browserStorage()): void {
  try {
    area?.setItem(STORAGE_KEY, JSON.stringify(book));
  } catch {
    // Storage blocked or full: the ranking just forgets, nothing breaks.
  }
}
