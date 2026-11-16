import { useEffect } from "react";
import { useWorkspace } from "../../../product/state/workspace";
import { worktreeLabel } from "./model";
import { useExplorer } from "./store";

/** The names a file tab shows for its project and worktree. They are read, never kept in the tab's id, so a rename shows at once. */
export function useFileTabNames(projectId: string, worktreeId: string, threadId: string | null): { project: string | null; worktree: string | null } {
  const project = useWorkspace((state) => state.projects.data?.find((one) => one.id === projectId)?.name ?? null);
  const kept = useExplorer((state) => state.worktrees[projectId]);
  useEffect(() => {
    if (!kept && projectId) void useExplorer.getState().loadWorktrees(projectId, { thread: threadId });
  }, [kept, projectId, threadId]);
  const found = kept?.data?.worktrees.find((one) => one.id === worktreeId);
  return { project, worktree: found ? worktreeLabel(found) : null };
}
