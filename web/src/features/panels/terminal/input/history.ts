import { useSyncExternalStore } from "react";
import { looksSecret } from "./classify";
import { agoWords } from "../../../../lib/format";

export interface HistoryEntry {
  command: string;
  /** Null when the shell reported no exit code. */
  ok: boolean | null;
  at: number;
}

export interface HistoryMatch {
  entry: HistoryEntry;
  /** What is shown: the command with anything secret-looking masked. */
  shown: string;
  /** The part of `shown` the query matched, as [start, end). */
  match: [number, number] | null;
}

const MASK = "••••••";
const BREAK = " ⏎ ";
const KEPT = 500;

/** A command as it may be put on screen: every word that looks like a credential, or its value, is masked. */
export function maskSecrets(text: string): string {
  return text.replace(/\S+/g, (word) => {
    const at = word.indexOf("=");
    if (at > 0 && looksSecret(word)) return `${word.slice(0, at + 1)}${MASK}`;
    return looksSecret(word) ? MASK : word;
  });
}

/** A command on one row: the secret-looking words are already masked, each line break becomes a visible separator. */
const oneRow = (text: string): string => text.replace(/\s*\n\s*/g, BREAK);

/**
 * A command as the shell may show it back: runs of whitespace and line breaks become one space, ends
 * trimmed. The shell reports the line it ran, which is the editor's line with its own spacing.
 */
const collapsed = (text: string): string => text.replace(/\s+/g, " ").trim();

const STORE_KEY = "orbyn.terminal.history.v1";
const MAX_KEYS = 50;
const MAX_BYTES = 1_000_000;
const MAX_COMMAND = 4096;
const MAX_KEY = 256;
// Control characters other than tab and a line feed (a multi-line command keeps its newlines); a carriage return and the
// rest are refused, since a stored line with one is not a command this editor wrote.
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u0008\u000b-\u001f\u007f]/;

const history = new Map<string, HistoryEntry[]>();
let loaded = false;

/** The one place the browser's storage is touched. Every read and write is guarded: blocked or throwing storage is a no-op. */
const storage = {
  read(): string | null {
    try {
      return window.localStorage.getItem(STORE_KEY);
    } catch {
      return null;
    }
  },
  write(value: string | null): void {
    try {
      if (value === null) window.localStorage.removeItem(STORE_KEY);
      else window.localStorage.setItem(STORE_KEY, value);
    } catch {
      // Blocked, full or private: the history stays in memory for this session.
    }
  },
};

/** The shape a stored line must have. A line is checked for secrets when it is recorded and again when it is loaded. */
const fits = (entry: HistoryEntry): boolean => entry.command.length <= MAX_COMMAND && !CONTROL.test(entry.command) && Number.isFinite(entry.at);

function readEntry(raw: unknown): HistoryEntry | null {
  if (typeof raw !== "object" || raw === null) return null;
  const { command, ok, at } = raw as Record<string, unknown>;
  if (typeof command !== "string" || typeof at !== "number" || !(typeof ok === "boolean" || ok === null)) return null;
  const entry = { command, ok, at };
  return command.trim() === "" || !fits(entry) || looksSecret(command) ? null : entry;
}

/** Reads the stored history once. Anything that is not the expected shape is dropped whole, never repaired. */
function load(): void {
  if (loaded) return;
  loaded = true;
  const text = storage.read();
  if (text === null) return;
  try {
    const data: unknown = JSON.parse(text);
    if (typeof data !== "object" || data === null || (data as { v?: unknown }).v !== 1) return;
    const table = (data as { h?: unknown }).h;
    if (typeof table !== "object" || table === null || Array.isArray(table)) return;
    const next = new Map<string, HistoryEntry[]>();
    for (const [key, list] of Object.entries(table)) {
      if (key === "" || key.length > MAX_KEY || !Array.isArray(list)) return;
      const entries: HistoryEntry[] = [];
      for (const raw of list) {
        const entry = readEntry(raw);
        if (entry) entries.push(entry);
      }
      if (entries.length > 0) next.set(key, entries.slice(-KEPT));
    }
    for (const key of [...next.keys()].slice(0, Math.max(0, next.size - MAX_KEYS))) next.delete(key);
    for (const [key, list] of next) history.set(key, list);
  } catch {
    // Not JSON: discard.
  }
}

/** Writes this device's history: only the thread-based key and the lines, within the key and size caps, oldest dropped first. */
function save(): void {
  const table = new Map<string, HistoryEntry[]>();
  for (const [key, list] of history) {
    const kept = key.length <= MAX_KEY ? list.filter(fits) : [];
    if (kept.length > 0) table.set(key, kept);
  }
  while (table.size > MAX_KEYS) table.delete(table.keys().next().value as string);
  const encode = () => JSON.stringify({ v: 1, h: Object.fromEntries(table) });
  let text = encode();
  while (text.length > MAX_BYTES && table.size > 0) {
    const [oldest, list] = table.entries().next().value as [string, HistoryEntry[]];
    if (table.size === 1) list.splice(0, Math.max(1, Math.ceil(list.length / 4)));
    else table.delete(oldest);
    if (list.length === 0) table.delete(oldest);
    text = encode();
  }
  storage.write(text);
}

/** Remembers a command run from the editor, per terminal, and keeps it on this device. A secret-looking line is never kept. */
export function recordCommand(key: string, command: string, now = Date.now()): void {
  const text = command.trim();
  if (text === "" || looksSecret(text)) return;
  load();
  // A new list, never the stored one changed in place: a row that reads it hears the change.
  const list = [...(history.get(key) ?? []), { command: text, ok: null, at: now }].slice(-KEPT);
  // Re-insert so the least recently used terminal is first in line to age out.
  history.delete(key);
  history.set(key, list);
  while (history.size > MAX_KEYS) history.delete(history.keys().next().value as string);
  save();
  changed();
}

/**
 * Remembers how a command the editor ran ended, on the oldest line of this terminal's history that is
 * still unknown and carries the same command. Takes the shell's own report (`shellOutcomes.ts`); a line the
 * shell never reported keeps no result, and says only when it ran. Returns whether a line took it.
 */
export function recordExitCode(key: string, command: string, exitCode: number): boolean {
  const wanted = collapsed(command);
  if (wanted === "") return false;
  load();
  const list = history.get(key);
  const at = list?.findIndex((entry) => entry.ok === null && collapsed(entry.command) === wanted) ?? -1;
  if (!list || at < 0) return false;
  const next = [...list];
  next[at] = { ...(list[at] as HistoryEntry), ok: exitCode === 0 };
  history.set(key, next);
  save();
  changed();
  return true;
}

/** The store's readers, so a result that arrives after the last render still shows. */
const listeners = new Set<() => void>();

const changed = () => {
  for (const listener of listeners) listener();
};

/** Calls back when a terminal's history gains a line or learns how a line ended. */
export function subscribeHistory(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

const NOTHING: readonly HistoryEntry[] = [];

export const historyOf = (key: string): readonly HistoryEntry[] => {
  load();
  return history.get(key) ?? NOTHING;
};

/** One terminal's history, for rendering: a row redrawn when the shell says how a line ended. */
export function useHistory(key: string): readonly HistoryEntry[] {
  return useSyncExternalStore(subscribeHistory, () => historyOf(key), () => NOTHING);
}

/** Newest first, one row per distinct command, filtered by a case-insensitive match on the masked text. */
export function searchHistory(entries: readonly HistoryEntry[], query: string, limit = 8): HistoryMatch[] {
  const needle = query.trim().toLowerCase();
  const seen = new Set<string>();
  const out: HistoryMatch[] = [];
  for (let at = entries.length - 1; at >= 0 && out.length < limit; at -= 1) {
    const entry = entries[at];
    if (!entry || seen.has(entry.command)) continue;
    seen.add(entry.command);
    // The whole entry is masked first, so a secret on any line is hidden before the lines are joined.
    const shown = oneRow(maskSecrets(entry.command));
    const found = needle === "" ? -1 : shown.toLowerCase().indexOf(needle);
    if (needle !== "" && found < 0) continue;
    out.push({ entry, shown, match: found < 0 ? null : [found, found + needle.length] });
  }
  return out;
}

/** The grey suggestion that follows the cursor: the rest of the newest earlier command that starts with the line. */
export function suggestion(entries: readonly HistoryEntry[], line: string): string {
  if (line.trim() === "" || line.includes("\n")) return "";
  for (let at = entries.length - 1; at >= 0; at -= 1) {
    const command = entries[at]?.command ?? "";
    // A grey suggestion is one line; a multi-line command is found through search instead.
    if (!command.includes("\n") && command.length > line.length && command.startsWith(line)) return command.slice(line.length);
  }
  return "";
}

/** The lines run in every terminal of the given threads, oldest first, as if they were one history. */
export function historyAcross(threadIds: readonly string[]): HistoryEntry[] {
  load();
  const out: HistoryEntry[] = [];
  for (const [key, list] of history) if (threadIds.some((id) => key.startsWith(`${id}:`))) out.push(...list);
  return out.sort((a, b) => a.at - b.at);
}

/**
 * What a history row says after its result mark: when it ran, and the count of failed runs the design's
 * `input-history` frame draws for a command that failed. A command the shell never reported says only when
 * it ran, so nothing claims a result that was not seen.
 */
export function rowDetail(entry: HistoryEntry, now = Date.now()): string {
  const when = agoWords(entry.at, now);
  return entry.ok === false ? `${when} · 1 failed` : when;
}
