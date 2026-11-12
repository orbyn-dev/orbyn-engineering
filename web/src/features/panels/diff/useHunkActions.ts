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

/** A hunk put back, which can be undone for a few seconds: the file as it was, and the revision the revert left. */
export interface Reverted {
  path: string;
  before: string;
  revision: string;
  seconds: number;
}

export const UNDO_SECONDS = 10;

const why = (failure: unknown) => (failure instanceof Error ? failure.message : toFailure(failure).message);

/**
 * Hunk and line edits of the working tree. Every write goes through the file route with the revision the
 * diff's file was read at, so a file that changed in between is refused and nothing is written.
 */
export function useHunkActions(threadId: string, cwd: string) {
  const [notice, setNotice] = useState<HunkNotice | null>(null);
  const [reverted, setReverted] = useState<Reverted | null>(null);
  const busy = useRef(false);
  const changed = useCallback(() => useThreadLogs.getState().touchDiff(threadId), [threadId]);

  useEffect(() => {
    if (!reverted) return;
    if (reverted.seconds <= 0) { setReverted(null); return; }
    const timer = setTimeout(() => setReverted((now) => (now ? { ...now, seconds: now.seconds - 1 } : now)), 1000);
    return () => clearTimeout(timer);
  }, [reverted]);

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
      setReverted({ path, before: done.before, revision: done.revision, seconds: UNDO_SECONDS });
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

  /** Puts the lines back as they were, if the file is still as the revert left it. */
  const undo = async () => {
    if (!reverted) return;
    const { path, before, revision } = reverted;
    setReverted(null);
    try {
      const inside = await checkoutPath(threadId, cwd, path);
      if (inside === null) throw new Error("That file is outside this thread's folder.");
      await saveThreadFile(threadId, { path: inside, content: before, base_revision: revision });
      changed();
    } catch (failure) {
      setNotice({
        title: "Couldn’t undo the revert.",
        detail: stale(failure) ? `${path} changed on disk.` : why(failure),
        action: "Refresh diff",
      });
    }
  };

  return { notice, reverted, revert, saveLine, undo, clearNotice: () => setNotice(null), dismissReverted: () => setReverted(null) };
}
