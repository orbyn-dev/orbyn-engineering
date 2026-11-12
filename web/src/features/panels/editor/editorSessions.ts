import { create } from "zustand";
import { toFailure } from "../../../lib/useAsync";
import { saveThreadFile } from "../api";
import { saveFailureReason } from "./editorModel";

/**
 * What an open file tab holds for the header and the tab strip: the text being edited, the text last saved and the
 * revision the edit started from. It lives here, not in the editor, so a tab that is switched away from and back keeps
 * its edits, and a closed one that is reopened offers them again.
 */
export interface EditorSession {
  path: string;
  text: string;
  saved: string;
  /** The revision the edit started from; null when the file does not exist (yet). */
  revision: string | null;
  saving: boolean;
  /** Why the last save did not happen, shown over the code. */
  trouble: Trouble | null;
}

type Trouble = { kind: "error"; reason: string } | { kind: "conflict"; revision: string | null };

interface Store {
  sessions: Record<string, EditorSession>;
  /** Tabs whose file cannot be written here (another worktree, a file that is not text): the header says Read-only. */
  locked: Record<string, true>;
  lock(id: string, on: boolean): void;
  put(id: string, session: EditorSession): void;
  patch(id: string, patch: Partial<EditorSession>): void;
  drop(id: string): void;
}

export const useEditorSessions = create<Store>((set) => ({
  sessions: {},
  locked: {},
  lock: (id, on) =>
    set((state) => {
      if (on === (id in state.locked)) return state;
      const { [id]: _gone, ...rest } = state.locked;
      return { locked: on ? { ...state.locked, [id]: true } : rest };
    }),
  put: (id, session) => set((state) => ({ sessions: { ...state.sessions, [id]: session } })),
  patch: (id, patch) =>
    set((state) => {
      const current = state.sessions[id];
      return current ? { sessions: { ...state.sessions, [id]: { ...current, ...patch } } } : state;
    }),
  drop: (id) =>
    set((state) => {
      if (!(id in state.sessions)) return state;
      const { [id]: _gone, ...rest } = state.sessions;
      return { sessions: rest };
    }),
}));

export const isDirty = (session: EditorSession | undefined): boolean => session !== undefined && session.text !== session.saved;


const writing = new Set<string>();

/**
 * Writes a tab's text through the service's write route, which checks the path and writes only if the file is still what
 * this edit started from. A refusal keeps the text and says why in the session. `against` replaces the revision to start
 * from (Keep mine: the disk's own, so the save overwrites it).
 */
export async function saveSession(threadId: string, tabId: string, against?: string | null): Promise<boolean> {
  const { sessions, patch } = useEditorSessions.getState();
  const current = sessions[tabId];
  if (!current || !isDirty(current)) return true;
  if (writing.has(tabId)) return false;
  writing.add(tabId);
  patch(tabId, { saving: true, trouble: null });
  const sent = current.text;
  try {
    const written = await saveThreadFile(threadId, { path: current.path, content: sent, base_revision: against === undefined ? current.revision : against });
    patch(tabId, { saved: sent, revision: written.revision, saving: false });
    return true;
  } catch (failure) {
    const known = toFailure(failure);
    const there = known.code === "file_conflict" ? (known.detail as { revision?: string | null } | undefined)?.revision : undefined;
    const trouble: Trouble =
      known.status === 409 && there === null
        ? { kind: "error", reason: "The file was deleted." }
        : known.status === 409
          ? { kind: "conflict", revision: there ?? null }
          : { kind: "error", reason: saveFailureReason(known) ?? known.message };
    patch(tabId, { saving: false, trouble });
    return false;
  } finally {
    writing.delete(tabId);
  }
}
