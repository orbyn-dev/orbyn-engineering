import type { Thread } from "../../../api/types";
import { ConfirmDialog } from "../../../ui/Dialog";
import { deleteThreads } from "./threadActions";

/** Confirms deleting one or more threads; the worktrees stay. */
export function DeleteThreadsDialog({ threads, onClose }: { threads: Thread[]; onClose(): void }) {
  return (
    <ConfirmDialog
      title={threads.length === 1 ? `Delete “${threads[0]?.title ?? "thread"}”?` : `Delete ${threads.length} threads?`}
      confirmLabel="Delete"
      tone="danger"
      onClose={onClose}
      onConfirm={() => deleteThreads(threads)}
    >
      <p>
        {threads.length === 1
          ? "Its messages and checkpoints are deleted. Code changes in its worktree stay."
          : "Their messages and checkpoints are deleted. Code changes in their worktrees stay."}
      </p>
    </ConfirmDialog>
  );
}
