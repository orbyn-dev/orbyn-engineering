import { useEffect, useState } from "react";
import { useNow } from "../../../lib/useNow";
import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { Ic } from "../shared/parts";
import { lessonsApi } from "./api";
import { clearLessonUndo, useLessonUndo } from "./undo";
import "./lessons.css";

/**
 * The ten seconds after a delete in which it can be taken back (KC5): the list's toast, counting the service's own
 * `undo_until` down and offering Undo. A window that runs out hides the toast; the service keeps the delete final, and an
 * Undo it no longer accepts (404) says why in a toast of its own. Any other failure says why and leaves Undo open until
 * the window runs out. Undo puts the lesson back and reads the list again.
 */
export function LessonUndoToast({ onUndone }: { onUndone?: () => void }) {
  const pending = useLessonUndo((state) => state.pending);
  const now = useNow(pending !== null, 1000);
  const [busy, setBusy] = useState(false);
  const left = pending ? Math.max(0, Math.ceil((pending.until - now) / 1000)) : 0;
  useEffect(() => {
    if (pending && left <= 0) clearLessonUndo(pending.id);
  }, [pending, left]);
  if (!pending || left <= 0) return null;
  const undo = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await lessonsApi.undo(pending.id);
      clearLessonUndo(pending.id);
      onUndone?.();
    } catch (error) {
      const one = toFailure(error);
      // Only the service saying the delete is final closes the window; a lost connection or a 5xx leaves Undo to try again.
      if (one.status === 404) clearLessonUndo(pending.id);
      useUi.getState().pushToast(one.message);
    } finally {
      setBusy(false);
    }
  };
  const share = Math.min(100, Math.max(0, (left / pending.seconds) * 100));
  return (
    <div className="toasts">
      <div className="toast cx-lstoast" role="status" aria-label={`Lesson deleted. Undo for ${pending.seconds} seconds`}>
        <Ic n="ok" sm />
        <span>Lesson deleted ·</span>
        <div className="cx-lsta">
          <button type="button" className="cx-lsundo" disabled={busy} onClick={() => void undo()}>
            {busy ? "Undoing…" : "Undo"}
          </button>
          <span className="cx-lsts">{left} s</span>
        </div>
        <span className="cx-lsbar" aria-hidden style={{ width: `${share}%` }} />
      </div>
    </div>
  );
}
