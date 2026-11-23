import { create } from "zustand";

/** The service keeps a deleted lesson undoable for ten seconds; the toast counts the window the answer named. */
export const UNDO_MS = 10_000;

/** A delete the person can still take back: the lesson's id, when its window shuts, and how long the window is. */
export interface LessonUndo {
  id: string;
  until: number;
  seconds: number;
}

interface LessonUndoState {
  pending: LessonUndo | null;
}

/**
 * The one delete whose Undo is still open. It lives outside the card because the card leaves the screen the moment the
 * delete answers: the list draws the toast, and the window is the service's own `undo_until`, not a guess here.
 */
export const useLessonUndo = create<LessonUndoState>(() => ({ pending: null }));

export const showLessonUndo = (pending: LessonUndo): void => useLessonUndo.setState({ pending });

export const clearLessonUndo = (): void => useLessonUndo.setState({ pending: null });
