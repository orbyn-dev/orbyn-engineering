import type { Thread } from "../../../api/types";
import { ConfirmDialog } from "../../../ui/Dialog";
import { useThreadRepos } from "../../repos";
import { deleteThreads } from "./threadActions";

/**
 * Confirms deleting one or more threads; the worktrees stay. One thread can work in several repositories, so a single
 * multi-repository thread says every repository's worktree stays. Its repositories are read without blocking the dialog;
 * several threads, one repository and a failed read keep the words the dialog had.
 */
export function DeleteThreadsDialog({ threads, onClose }: { threads: Thread[]; onClose(): void }) {
  // Only a single thread can be a multi-repository one, and this read writes nothing.
  const { repos } = useThreadRepos(threads[0]?.id ?? "", 0, threads.length === 1);
  const manyRepositories = threads.length === 1 && (repos?.length ?? 0) > 1;
  const words = threads.length === 1
    ? manyRepositories
      ? "Its messages and checkpoints are deleted. Code changes in every repository's worktree stay."
      : "Its messages and checkpoints are deleted. Code changes in its worktree stay."
    : "Their messages and checkpoints are deleted. Code changes in their worktrees stay.";
  return (
    <ConfirmDialog
      title={threads.length === 1 ? `Delete “${threads[0]?.title ?? "thread"}”?` : `Delete ${threads.length} threads?`}
      confirmLabel="Delete"
      tone="danger"
      onClose={onClose}
      onConfirm={() => deleteThreads(threads)}
    >
      <p>{words}</p>
    </ConfirmDialog>
  );
}
