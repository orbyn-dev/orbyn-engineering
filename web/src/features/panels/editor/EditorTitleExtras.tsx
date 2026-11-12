import { Check, Lock } from "lucide-react";
import { Button } from "../../../ui/controls";
import { isDirty, useEditorSessions } from "./editorSessions";

/** Pressing Save turns the button off, which would drop focus with it: the cursor goes back to the code. */
const backToCode = () => {
  if (document.activeElement === document.body || document.activeElement?.closest(".ctx-title")) document.querySelector<HTMLTextAreaElement>(".ed-ta")?.focus();
};

/**
 * What the line under the tab strip adds for an open file: Read-only when the file cannot be written here, and for an
 * editable one Unsaved and Save (⌘S). Save is a check beside the menu in a narrow panel.
 */
export function EditorTitleExtras({ tabId, onSave }: { tabId: string; onSave(): Promise<unknown> | void }) {
  const session = useEditorSessions((state) => state.sessions[tabId]);
  const locked = useEditorSessions((state) => tabId in state.locked);
  if (locked) {
    return (
      <span className="ed-pill">
        <Lock size={12} aria-hidden />
        Read-only
      </span>
    );
  }
  if (!session) return null;
  const dirty = isDirty(session);
  return (
    <span className="ed-title-end">
      {dirty ? <span className="ed-unsaved">Unsaved</span> : null}
      <Button className="ed-save" small tone={dirty ? "primary" : "quiet"} disabled={!dirty || session.saving} title="Save ⌘S" aria-label="Save" onClick={() => void Promise.resolve(onSave()).then(backToCode)}>
        <span className="ed-save-text">Save</span>
        <Check className="ed-save-check" size={14} aria-hidden />
      </Button>
    </span>
  );
}
