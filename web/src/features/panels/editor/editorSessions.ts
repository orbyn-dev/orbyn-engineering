import { create } from "zustand";
import { toFailure } from "../../../lib/useAsync";
import { saveThreadFile } from "../api";
import { saveFailureReason, withEol } from "./editorModel";

/**
 * What an open file holds for the header and the tab strip: the text being edited, the text last saved and the revision
 * the edit started from. It lives here, not in the editor, so a tab that is switched away from and back keeps its edits.
 * A session belongs to one thread's checkout and one path: it is found by those, never by a tab id alone (a thread file's
 * tab id names no thread), and it saves into the thread it came from.
 */
export interface EditorSession {
  key: string;
  threadId: string;
  cwd: string;
  path: string;
  /** The tabs showing this file, so the strip can mark them. */
  tabs: string[];
  /** The text as the editor holds it, with every line break a line feed. */
  text: string;
  saved: string;
  /** The file's own line ending, put back when it is written. */
  eol: "LF" | "CRLF";
  /** The revision the edit started from; null when the file does not exist (yet). */
  revision: string | null;
  saving: boolean;
  /** Why the last save did not happen, shown over the code. */
  trouble: Trouble | null;
}

type Trouble = { kind: "error"; reason: string } | { kind: "conflict"; revision: string | null };

interface Store {
  sessions: Record<string, EditorSession>;
  /** Tabs whose file cannot be written here (another worktree, a locked one, a file that is not text): the header says Read-only. */
  locked: Record<string, true>;
  lock(threadId: string, tabId: string, on: boolean): void;
  put(session: EditorSession): void;
  patch(key: string, patch: Partial<EditorSession>): void;
  drop(key: string): void;
}

export const fileKey = (threadId: string, cwd: string, path: string) => JSON.stringify([threadId, cwd, path]);
const lockKey = (threadId: string, tabId: string) => JSON.stringify([threadId, tabId]);

export const useEditorSessions = create<Store>((set) => ({
  sessions: {},
  locked: {},
  lock: (threadId, tabId, on) =>
    set((state) => {
      const key = lockKey(threadId, tabId);
      if (on === (key in state.locked)) return state;
      const { [key]: _gone, ...rest } = state.locked;
      return { locked: on ? { ...state.locked, [key]: true } : rest };
    }),
  put: (session) => set((state) => ({ sessions: { ...state.sessions, [session.key]: session } })),
  patch: (key, patch) =>
    set((state) => {
      const current = state.sessions[key];
      return current ? { sessions: { ...state.sessions, [key]: { ...current, ...patch } } } : state;
    }),
  drop: (key) =>
    set((state) => {
      if (!(key in state.sessions)) return state;
      const { [key]: _gone, ...rest } = state.sessions;
      return { sessions: rest };
    }),
}));

export const isDirty = (session: EditorSession | undefined): boolean => session !== undefined && session.text !== session.saved;

/** The session a tab of this thread's checkout shows, if any. */
export const sessionOfTab = (sessions: Record<string, EditorSession>, threadId: string, cwd: string, tabId: string): EditorSession | undefined =>
  Object.values(sessions).find((session) => session.threadId === threadId && session.cwd === cwd && session.tabs.includes(tabId));

export const isLocked = (locked: Record<string, true>, threadId: string, tabId: string) => lockKey(threadId, tabId) in locked;

/** The file as it is on disk: the saved text with its own line ending. */
export const savedContents = (session: EditorSession) => withEol(session.saved, session.eol);

const writing = new Set<string>();

/**
 * Writes a session's text through the service's write route, which checks the path and writes only if the file is still
 * what this edit started from, into the thread the session came from. A refusal keeps the text and says why in the
 * session. `against` replaces the revision to start from (Keep mine: the disk's own, so the save overwrites it).
 */
export async function saveSession(key: string, against?: string | null): Promise<boolean> {
  const { sessions, patch } = useEditorSessions.getState();
  const current = sessions[key];
  if (!current || !isDirty(current)) return true;
  if (writing.has(key)) return false;
  writing.add(key);
  patch(key, { saving: true, trouble: null });
  const sent = current.text;
  try {
    const written = await saveThreadFile(current.threadId, { path: current.path, content: withEol(sent, current.eol), base_revision: against === undefined ? current.revision : against });
    patch(key, { saved: sent, revision: written.revision, saving: false });
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
    patch(key, { saving: false, trouble });
    return false;
  } finally {
    writing.delete(key);
  }
}

// Leaving with unsaved text asks first.
if (typeof window !== "undefined") {
  const ask = (event: BeforeUnloadEvent) => {
    event.preventDefault();
    event.returnValue = "";
  };
  let on = false;
  useEditorSessions.subscribe((state) => {
    const dirty = Object.values(state.sessions).some(isDirty);
    if (dirty === on) return;
    on = dirty;
    if (dirty) window.addEventListener("beforeunload", ask);
    else window.removeEventListener("beforeunload", ask);
  });
}

