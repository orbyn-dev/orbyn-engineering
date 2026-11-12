import { AlertTriangle, Lock } from "lucide-react";
import { useEffect, useLayoutEffect } from "react";
import type { FileContent } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { Button } from "../../../ui/controls";
import { readThreadFile } from "../api";
import { EditorSurface } from "./EditorSurface";
import { fileKey, isDirty, saveSession, useEditorSessions } from "./editorSessions";
import { endingLabel, endingsOf, toLF } from "./editorModel";

/**
 * A text file of the thread's own checkout, edited in place (editor-default, -unsaved, -conflict, -save-error). Saving goes
 * through the service's write route, which checks the path and writes only if the file is still what this edit started
 * from; nothing is saved on its own.
 */
export function FileEditor({ threadId, tabId, path, file, line, cwd, locked = false, onReload }: { threadId: string; tabId: string; path: string; file: FileContent; line: number | null; cwd: string; /** The worktree is locked: the file reads, and nothing is saved. */ locked?: boolean; onReload(): void }) {
  const key = fileKey(threadId, cwd, path);
  const session = useEditorSessions((state) => state.sessions[key]);
  // A session that goes missing while the file is showing (given up on a close that was then cancelled) is made again.
  const missing = session === undefined;
  // A session of this file from an earlier visit carries unsaved text; a clean one is replaced by what the file holds now.
  useLayoutEffect(() => {
    if (locked) return;
    const kept = useEditorSessions.getState().sessions[key];
    const tabs = [...new Set([...(kept?.tabs ?? []), tabId])];
    if (kept && isDirty(kept)) {
      useEditorSessions.getState().patch(key, { tabs });
      return;
    }
    useEditorSessions.getState().put({
      key, threadId, cwd, path, tabs,
      text: toLF(file.content), saved: toLF(file.content), ...endingsOf(file.content),
      revision: file.revision, saving: false, trouble: null,
    });
  }, [key, tabId, threadId, cwd, path, file.content, file.revision, locked, missing]);
  const { patch } = useEditorSessions.getState();
  const text = session?.text ?? toLF(file.content);

  useEffect(
    () => () => {
      const now = useEditorSessions.getState().sessions[key];
      // A clean file leaves nothing behind; a dirty one keeps its text for when it is shown or reopened.
      if (!isDirty(now)) useEditorSessions.getState().drop(key);
    },
    [key],
  );
  useEffect(() => {
    useEditorSessions.getState().lock(threadId, tabId, locked);
    return () => useEditorSessions.getState().lock(threadId, tabId, false);
  }, [threadId, tabId, locked]);
  const trouble = session?.trouble ?? null;

  const reload = async () => {
    try {
      const fresh = await readThreadFile(threadId, path);
      patch(key, { text: toLF(fresh.content), saved: toLF(fresh.content), ...endingsOf(fresh.content), revision: fresh.revision, trouble: null });
      onReload();
    } catch (failure) {
      patch(key, { trouble: { kind: "error", reason: toFailure(failure).message } });
    }
  };
  // Keep mine: the next save overwrites what is on disk, so it starts from the revision the disk holds now.
  const keepMine = () => void saveSession(key, trouble?.kind === "conflict" ? trouble.revision : undefined);

  const dirty = session ? isDirty(session) : false;
  const ends = session ?? endingsOf(file.content);
  if (locked) {
    return (
      <EditorSurface
        path={path}
        text={file.content}
        reveal={line}
        standing="read-only"
        banner={
          <div className="x2banner" role="status">
            <Lock size={14} aria-hidden />
            <span>This worktree is locked. Files open read-only.</span>
          </div>
        }
      />
    );
  }
  return (
    <EditorSurface
      path={path}
      text={text}
      eolLabel={endingLabel(ends.eol, ends.mixed)}
      reveal={line}
      comments={{ threadId, cwd, revision: session?.revision ?? file.revision, text: session?.saved ?? toLF(file.content) }}
      standing={dirty ? "unsaved" : "saved"}
      onChange={(next) => patch(key, { text: next })}
      onSave={() => void saveSession(key)}
      banner={
        trouble?.kind === "error" ? (
          <div className="ed-banner" data-tone="error" role="alert">
            <AlertTriangle size={14} aria-hidden />
            <div>
              <b>Couldn’t save {path}.</b> {trouble.reason} Your changes are still here.
            </div>
            <div className="ed-banner-end">
              <Button small onClick={() => void saveSession(key)}>
                Try again
              </Button>
            </div>
          </div>
        ) : trouble?.kind === "endings" ? (
          <div className="ed-banner" role="alert">
            <AlertTriangle size={14} aria-hidden />
            <div>
              <b>This file mixes line endings.</b> Saving writes them all as {session?.eol ?? "LF"}.
            </div>
            <div className="ed-banner-end">
              <Button small tone="quiet" onClick={() => patch(key, { trouble: null })}>
                Cancel
              </Button>
              <Button small onClick={() => void saveSession(key, undefined, true)}>
                Save anyway
              </Button>
            </div>
          </div>
        ) : trouble?.kind === "conflict" ? (
          <div className="ed-banner" role="alert">
            <AlertTriangle size={14} aria-hidden />
            <div>
              <b>Changed on disk</b> since you opened it.
            </div>
            <div className="ed-banner-end">
              <Button small tone="quiet" onClick={keepMine}>
                Keep mine
              </Button>
              <Button small onClick={() => void reload()}>
                Reload
              </Button>
            </div>
          </div>
        ) : null
      }
    />
  );
}
