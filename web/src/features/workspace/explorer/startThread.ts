import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { useDraftStore, worktreePurpose } from "../../../product/state/draftStore";
import { navigate } from "../../../product/state/route";

/** Opens a new thread's composer for a project, working in the worktree (the primary checkout needs no choice). */
export function startThreadIn(projectId: string, worktree: ExplorerWorktree) {
  useDraftStore.getState().setReuseWorktree(worktree.is_main ? null : worktreePurpose(worktree.path));
  navigate({ name: "new", projectId });
}
