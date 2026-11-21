import { useCallback, useEffect, useMemo } from "react";
import { useConnectionKind } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { levelKey, useExplorer } from "../../../workspace";
import type { PathEntry } from "./completions";
import type { EngineTarget } from "./route";

/** The thread's own engine and model, and whether it can take a prompt right now. */
export function useEngineTarget(threadId: string): EngineTarget & { engineId: string } {
  const thread = useWorkspace((state) => state.threads[threadId]);
  const engines = useWorkspace((state) => state.engines.data);
  const connection = useConnectionKind();
  const engine = engines?.find((entry) => entry.id === thread?.engine);
  const availability: EngineTarget["availability"] =
    connection !== null || (engine && !engine.installed) ? "offline" : engine?.auth === "logged_out" ? "signed-out" : "ready";
  return { engineId: thread?.engine ?? "", name: engine?.name ?? thread?.engine ?? "the engine", model: thread?.model ?? null, availability };
}

/**
 * Folder listings for path completion, read from the explorer's own cache (the same reads the Files tree makes). The
 * paths are relative to the thread's worktree root; the shell's current folder is not tracked.
 * `want` is the folder the next completion needs, so it is read when it is not yet there.
 */
export function usePathListing(threadId: string, want: string | null) {
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id ?? null);
  const worktrees = useExplorer((state) => (projectId ? state.worktrees[projectId]?.data?.worktrees : undefined));
  const levels = useExplorer((state) => state.levels);
  const worktreeId = useMemo(() => worktrees?.find((tree) => tree.threads.some((thread) => thread.id === threadId))?.id ?? null, [worktrees, threadId]);
  const path = want === null ? null : want.replace(/\/$/, "");
  useEffect(() => {
    if (!projectId) return;
    const store = useExplorer.getState();
    if (!worktrees) void store.loadWorktrees(projectId, { thread: threadId, quiet: true });
    else if (worktreeId && path !== null && !store.levels[levelKey(projectId, worktreeId, path)]) {
      void store.loadLevel(projectId, worktreeId, path, { quiet: true });
    }
  }, [projectId, worktrees, worktreeId, path, threadId]);
  return useCallback(
    (dir: string): readonly PathEntry[] | null => {
      if (!projectId || !worktreeId) return null;
      const entries = levels[levelKey(projectId, worktreeId, dir.replace(/\/$/, ""))]?.data?.entries;
      return entries ? entries.map((entry) => ({ name: entry.name, kind: entry.kind })) : null;
    },
    [projectId, worktreeId, levels],
  );
}
