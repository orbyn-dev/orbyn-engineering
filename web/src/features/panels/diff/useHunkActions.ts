import { useCallback, useEffect, useRef, useState } from "react";
import { toFailure } from "../../../lib/useAsync";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { readThreadFile, saveThreadFile } from "../api";
import { checkoutPath } from "./openChanged";
import { editedContent, revertedContent, type Hunk } from "./hunkModel";

/** The message above the file list when a hunk action did not happen, with the one button that mends it. */
export interface HunkNotice {
  title: string;
  detail: string;
  action: "Refresh diff" | "Reload diff";
}

/**
 * A hunk put back, which can be undone for a few seconds: the file as it was, and the revision the revert left.
 * It belongs to the thread and folder it was made in, whatever the panel shows by the time Undo is pressed.
 */
export interface Reverted {
  threadId: string;
  cwd: string;
  path: string;
  before: string;
  revision: string;
  /** When the undo runs out, in milliseconds. */
  until: number;
}

export const UNDO_SECONDS = 10;

const why = (failure: unknown) => (failure instanceof Error ? failure.message : toFailure(failure).message);

/**
 * Hunk and line edits of the working tree. Every write goes through the file route with the revision of the
 * file just read. The lines the diff showed must still be there, which is what catches a file that moved on;
 * the revision guard then only covers a change between that read and the write.
 */
export function useHunkActions(threadId: string, cwd: string) {
  const [notice, setNotice] = useState<HunkNotice | null>(null);
  // Every revert still open to undo, oldest first. The message shows the newest; each keeps its own undo.
  const [open, setOpen] = useState<Reverted[]>([]);
  const [now, setNow] = useState(() => Date.now());
  const busy = useRef(false);
  const changed = useCallback((id: string = threadId) => useThreadLogs.getState().touchDiff(id), [threadId]);

  // Another thread is another working tree: nothing from this one carries over.
  useEffect(() => {
    setNotice(null);
    setOpen([]);
  }, [threadId]);
  useEffect(() => {
    if (open.length === 0) return;
    const timer = setTimeout(() => {
      const at = Date.now();
      setNow(at);
      setOpen((list) => list.filter((entry) => entry.until > at));
    }, 1000);
    return () => clearTimeout(timer);
  }, [open, now]);
  const shown = open.at(-1) ?? null;
  const reverted = shown ? { path: shown.path, seconds: Math.max(0, Math.ceil((shown.until - now) / 1000)) } : null;

  /** Reads the file the diff names; the lines the diff showed must still be there for `change` to answer. */
  const write = async (path: string, change: (content: string) => string | null) => {
    const inside = await checkoutPath(threadId, cwd, path);
    if (inside === null) throw new Error("That file is outside this thread's folder.");
    const file = await readThreadFile(threadId, inside);
    if (file.binary || file.truncated) throw new Error("That file can't be edited here.");
    const next = change(file.content);
    if (next === null) return null;
    const saved = await saveThreadFile(threadId, { path: inside, content: next, base_revision: file.revision }) as { revision: string };
    return { before: file.content, revision: saved.revision };
  };
  const stale = (failure: unknown) => toFailure(failure).status === 409;

  const revert = async (path: string, hunk: Hunk) => {
    if (busy.current) return;
    busy.current = true;
    setNotice(null);
    try {
      const done = await write(path, (content) => revertedContent(content, hunk));
      if (done === null) throw Object.assign(new Error("stale"), { stale: true });
      const at = Date.now();
      setNow(at);
      setOpen((list) => [...list, { threadId, cwd, path, before: done.before, revision: done.revision, until: at + UNDO_SECONDS * 1000 }]);
      changed();
    } catch (failure) {
      const old = stale(failure) || (failure as { stale?: boolean }).stale === true;
      setNotice({
        title: "Couldn’t revert the hunk.",
        detail: old ? "The file has changed since this diff was read. Nothing was changed." : `${why(failure)} Nothing was changed.`,
        action: "Refresh diff",
      });
    } finally {
      busy.current = false;
    }
  };

  /** Writes one line; true when it was written. The caller keeps the typed text when it was not. */
  const saveLine = async (path: string, number: number, was: string, next: string) => {
    if (busy.current) return false;
    busy.current = true;
    setNotice(null);
    try {
      const done = await write(path, (content) => editedContent(content, number, was, next));
      if (done === null) throw Object.assign(new Error("stale"), { stale: true });
      changed();
      return true;
    } catch (failure) {
      const old = stale(failure) || (failure as { stale?: boolean }).stale === true;
      setNotice({
        title: "Couldn’t save the line.",
        detail: old ? `${path} changed on disk. Reload the diff, then edit again.` : why(failure),
        action: "Reload diff",
      });
      return false;
    } finally {
      busy.current = false;
    }
  };

  /** Puts the lines back as they were, in the thread the revert was made in, if the file is still as the revert left it. */
  const undo = async () => {
    const entry = open.at(-1);
    if (!entry || busy.current) return;
    busy.current = true;
    setOpen((list) => list.filter((item) => item !== entry));
    try {
      const inside = await checkoutPath(entry.threadId, entry.cwd, entry.path);
      if (inside === null) throw new Error("That file is outside this thread's folder.");
      await saveThreadFile(entry.threadId, { path: inside, content: entry.before, base_revision: entry.revision });
      changed(entry.threadId);
    } catch {
      // The same pattern as a failed revert: it says so, and that nothing was changed.
      if (entry.threadId === threadId) setNotice({ title: "Couldn’t undo the revert.", detail: "Nothing was changed.", action: "Refresh diff" });
    } finally {
      busy.current = false;
    }
  };

  return { notice, reverted, revert, saveLine, undo, clearNotice: () => setNotice(null), dismissReverted: () => setOpen((list) => list.slice(0, -1)) };
}
