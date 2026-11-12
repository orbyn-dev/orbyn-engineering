import { AlertTriangle } from "lucide-react";
import { useEffect, useLayoutEffect } from "react";
import type { FileContent } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { Button } from "../../../ui/controls";
import { readThreadFile } from "../api";
import { EditorSurface } from "./EditorSurface";
import { isDirty, saveSession, useEditorSessions } from "./editorSessions";

/**
 * A text file of the thread's own checkout, edited in place (editor-default, -unsaved, -conflict, -save-error). Saving goes
 * through the service's write route, which checks the path and writes only if the file is still what this edit started
 * from; nothing is saved on its own.
 */
export function FileEditor({ threadId, tabId, path, file, line, onReload }: { threadId: string; tabId: string; path: string; file: FileContent; line: number | null; onReload(): void }) {
  const session = useEditorSessions((state) => state.sessions[tabId]);
  // A session from an earlier visit carries unsaved text; a clean one is replaced by what the file holds now.
  useLayoutEffect(() => {
    const kept = useEditorSessions.getState().sessions[tabId];
    if (!kept || kept.path !== path || !isDirty(kept)) {
      useEditorSessions.getState().put(tabId, { path, text: file.content, saved: file.content, revision: file.revision, saving: false, trouble: null });
    }
  }, [tabId, path, file.content, file.revision]);
  const { patch } = useEditorSessions.getState();
  const text = session?.text ?? file.content;

  useEffect(
    () => () => {
      const now = useEditorSessions.getState().sessions[tabId];
      // A clean tab leaves nothing behind; a dirty one keeps its text for when it is shown or reopened.
      if (!isDirty(now)) useEditorSessions.getState().drop(tabId);
    },
    [tabId],
  );
  const trouble = session?.trouble ?? null;

  const reload = async () => {
    try {
      const fresh = await readThreadFile(threadId, path);
      patch(tabId, { text: fresh.content, saved: fresh.content, revision: fresh.revision, trouble: null });
      onReload();
    } catch (failure) {
      patch(tabId, { trouble: { kind: "error", reason: toFailure(failure).message } });
    }
  };
  // Keep mine: the next save overwrites what is on disk, so it starts from the revision the disk holds now.
  const keepMine = () => void saveSession(threadId, tabId, trouble?.kind === "conflict" ? trouble.revision : undefined);

  const dirty = session ? isDirty(session) : false;
  return (
    <EditorSurface
      path={path}
      text={text}
      reveal={line}
      standing={dirty ? "unsaved" : "saved"}
      onChange={(next) => patch(tabId, { text: next })}
      onSave={() => void saveSession(threadId, tabId)}
      banner={
        trouble?.kind === "error" ? (
          <div className="ed-banner" data-tone="error" role="alert">
            <AlertTriangle size={14} aria-hidden />
            <div>
              <b>Couldn’t save {path}.</b> {trouble.reason} Your changes are still here.
            </div>
            <div className="ed-banner-end">
              <Button small onClick={() => void saveSession(threadId, tabId)}>
                Try again
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
