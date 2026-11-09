import { api } from "../../../api/client";
import { useThreadLogs } from "../../../state/threadLogs";
import { useUi } from "../../../state/ui";
import { ConfirmDialog } from "../../../ui/Dialog";

/** Confirms, then puts the folder back to how it was before a turn. */
export function RestoreFilesDialog({ threadId, runId, onClose }: { threadId: string; runId: string; onClose(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  return (
    <ConfirmDialog
      title="Restore files?"
      confirmLabel="Restore files"
      onClose={onClose}
      onConfirm={async () => {
        await api.restoreWorkspace(threadId, runId, "start");
        useThreadLogs.getState().touchDiff(threadId);
        pushToast("Files restored");
      }}
    >
      <p>The folder goes back to how it was before this turn. What it holds now is saved first, so this can be undone from Git.</p>
    </ConfirmDialog>
  );
}
