import { useEffect } from "react";
import { Button } from "../../../ui/controls";
import { Dialog } from "../../../ui/Dialog";

/**
 * editor-close-confirm: closing a file with unsaved changes asks once. Save is the default, ⌘↵ confirms it and Esc cancels.
 * With several unsaved files the caller asks again for each, naming the file.
 */
export function SaveChangesDialog({ path, onSave, onDiscard, onCancel }: { path: string; onSave(): void; onDiscard(): void; onCancel(): void }) {
  const name = path.split("/").at(-1) ?? path;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        onSave();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onSave]);
  return (
    <Dialog
      title={`Save changes to ${name}?`}
      onClose={onCancel}
      footer={
        <>
          <Button tone="quiet" onClick={onCancel}>
            Cancel
          </Button>
          <Button tone="quiet" onClick={onDiscard}>
            Don’t save
          </Button>
          <Button tone="primary" data-dialog-focus onClick={onSave}>
            Save
          </Button>
        </>
      }
    >
      <p>Your changes to {path} will be lost if you don’t save them.</p>
    </Dialog>
  );
}
