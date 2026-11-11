import type { Thread } from "../../../api/types";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { useDraftStore, worktreePurpose } from "../../../product/state/draftStore";
import { navigate } from "../../../product/state/route";
import { samePath } from "./model";

/**
 * Whether a new thread can be started in a worktree. The primary checkout always can. A linked one can only when Orbyn made it:
 * a draft reuses a worktree by the purpose its folder was made from, which only a worktree Orbyn made (one a thread of ours
 * runs or ran in) is known to have. Another worktree is not guessed at; starting a thread in one by its id is a follow-up.
 */
export function canStartThreadIn(worktree: ExplorerWorktree, threads: readonly Thread[]): boolean {
  if (worktree.is_main) return true;
  return threads.some((thread) => thread.worktree !== null && samePath(thread.worktree.path, worktree.path));
}

/** Opens a new thread's composer for a project, working in the worktree: the primary checkout itself, or the reused linked one. */
export function startThreadIn(projectId: string, worktree: ExplorerWorktree) {
  const drafts = useDraftStore.getState();
  drafts.setReuseWorktree(worktree.is_main ? null : worktreePurpose(worktree.path));
  // Without this the device default would decide, and may make a new worktree.
  drafts.setCurrentCheckout(worktree.is_main);
  navigate({ name: "new", projectId });
}
