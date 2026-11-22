/**
 * How the shell said each command it finished ended. The codes come from the shell's own OSC 133/633
 * marks (`shellMarks.ts`), which the terminal reads as the output arrives; nothing here asks a shell for
 * anything. Orbyn installs no shell integration, so a shell that emits no marks reports nothing and the
 * editor says only when a line ran.
 *
 * The command text is the shell's, so it is kept as data: never shown, never logged, never a path. The
 * editor shows a line of its own history, and this only says how that line ended.
 */
import { useSyncExternalStore } from "react";
import type { FinishedCommand } from "./shellMarks";

/** The commands one terminal reported, newest last: enough for the editor's last runs. */
export interface ReportedCommands {
  /** The sequence number of the newest command published, -1 before the first. */
  lastSeq: number;
  kept: readonly FinishedCommand[];
}

const KEPT = 200;
const NOTHING: readonly FinishedCommand[] = [];

const reported = new Map<string, ReportedCommands>();
const listeners = new Map<string, Set<() => void>>();

const tell = (key: string) => {
  for (const listener of listeners.get(key) ?? []) listener();
};

/**
 * Keeps the commands `finished` reports that this terminal has not published yet, and drops the oldest
 * past `KEPT`. Takes the whole list each time, so the caller needs no memory of what it sent before.
 */
export function publishFinished(key: string, finished: readonly FinishedCommand[]): void {
  const before = reported.get(key);
  const lastSeq = before?.lastSeq ?? -1;
  const fresh = finished.filter((command) => command.seq > lastSeq);
  const newest = fresh.at(-1);
  if (!newest) return;
  const kept = [...(before?.kept ?? NOTHING), ...fresh].slice(-KEPT);
  reported.set(key, { lastSeq: newest.seq, kept });
  tell(key);
}

/** The commands this terminal has reported ending, oldest first. */
export const reportedCommandsOf = (key: string): readonly FinishedCommand[] => reported.get(key)?.kept ?? NOTHING;

/** Forgets a terminal's commands: a closed terminal, or one that starts again, leaves nothing behind. */
export function clearReportedCommands(key: string): void {
  if (reported.delete(key)) tell(key);
}

export function subscribeReportedCommands(key: string, listener: () => void): () => void {
  const set = listeners.get(key) ?? new Set();
  listeners.set(key, set);
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0 && listeners.get(key) === set) listeners.delete(key);
  };
}

/** The commands a terminal reported ending, for rendering. */
export function useReportedCommands(key: string): readonly FinishedCommand[] {
  return useSyncExternalStore(
    (listener) => subscribeReportedCommands(key, listener),
    () => reportedCommandsOf(key),
    () => NOTHING,
  );
}
