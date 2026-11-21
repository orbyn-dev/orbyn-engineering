import { looksSecret } from "./classify";

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
const KEPT = 500;

/** A command as it may be put on screen: every word that looks like a credential, or its value, is masked. */
export function maskSecrets(text: string): string {
  return text.replace(/\S+/g, (word) => {
    const at = word.indexOf("=");
    if (at > 0 && looksSecret(word)) return `${word.slice(0, at + 1)}${MASK}`;
    return looksSecret(word) ? MASK : word;
  });
}

const history = new Map<string, HistoryEntry[]>();

/** Remembers a command run from the editor, for this terminal only and this session. A secret-looking line is never kept. */
export function recordCommand(key: string, command: string, now = Date.now()): void {
  const text = command.trim();
  if (text === "" || looksSecret(text)) return;
  const list = history.get(key) ?? [];
  list.push({ command: text, ok: null, at: now });
  if (list.length > KEPT) list.splice(0, list.length - KEPT);
  history.set(key, list);
}

export const historyOf = (key: string): readonly HistoryEntry[] => history.get(key) ?? [];

/** Newest first, one row per distinct command, filtered by a case-insensitive match on the masked text. */
export function searchHistory(entries: readonly HistoryEntry[], query: string, limit = 8): HistoryMatch[] {
  const needle = query.trim().toLowerCase();
  const seen = new Set<string>();
  const out: HistoryMatch[] = [];
  for (let at = entries.length - 1; at >= 0 && out.length < limit; at -= 1) {
    const entry = entries[at];
    if (!entry || seen.has(entry.command)) continue;
    seen.add(entry.command);
    const shown = maskSecrets(entry.command);
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
    if (command.length > line.length && command.startsWith(line)) return command.slice(line.length);
  }
  return "";
}
