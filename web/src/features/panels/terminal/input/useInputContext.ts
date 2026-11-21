import { useCallback, useEffect, useMemo } from "react";
import { connectionKind, useConnection, useConnectionKind } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { levelKey, useExplorer } from "../../../workspace";
import type { EngineStatus } from "../../../../api/types";
import type { PathEntry } from "./completions";
import type { EngineTarget } from "./route";

/** Where an engine stands for a prompt: the device or service is out of reach, the engine is missing, or it is signed out. */
const availabilityOf = (offline: boolean, engine: { installed: boolean; auth: string } | undefined): EngineTarget["availability"] =>
  offline || (engine && !engine.installed) ? "offline" : engine?.auth === "logged_out" ? "signed-out" : "ready";

export type EngineView = EngineTarget & { engineId: string; /** The engine's own sign-in command, when it has one. */ signInCommand: string | null };

/** The thread's engine as the stores hold it right now, for a check made after an await (a hook would be stale). */
export function readEngineTarget(threadId: string): EngineView {
  const { threads, engines } = useWorkspace.getState();
  const thread = threads[threadId];
  return viewOf(thread, engines.data ?? undefined, connectionKind(useConnection.getState()) !== null);
}

function viewOf(thread: { engine: string; model: string | null } | undefined, engines: readonly EngineStatus[] | undefined, offline: boolean): EngineView {
  const engine = engines?.find((entry) => entry.id === thread?.engine);
  return {
    engineId: thread?.engine ?? "",
    name: engine?.name ?? thread?.engine ?? "the engine",
    model: thread?.model ?? null,
    availability: availabilityOf(offline, engine),
    signInCommand: engine?.sign_in_command ?? null,
  };
}

/** The thread's own engine and model, and whether it can take a prompt right now. */
export function useEngineTarget(threadId: string): EngineView {
  const thread = useWorkspace((state) => state.threads[threadId]);
  const engines = useWorkspace((state) => state.engines.data);
  const connection = useConnectionKind();
  return viewOf(thread, engines ?? undefined, connection !== null);
}

/** What reading the folder the next completion needs is doing. `idle` is nothing to wait for, or it is there. */
export type PathState = "idle" | "loading" | "failed";

/**
 * Folder listings for path completion, read from the explorer's own cache (the same reads the Files tree makes). The
 * paths are relative to the thread's worktree root; the shell's current folder is not tracked.
 * `want` is the folder the next completion needs, so it is read when it is not yet there; `state` says how that read is going.
 */
export function usePathListing(threadId: string, want: string | null): { listDir: (dir: string) => readonly PathEntry[] | null; state: PathState } {
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id ?? null);
  const worktrees = useExplorer((state) => (projectId ? state.worktrees[projectId]?.data?.worktrees : undefined));
  const treesFailed = useExplorer((state) => (projectId ? state.worktrees[projectId]?.error != null : false));
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
  const listDir = useCallback(
    (dir: string): readonly PathEntry[] | null => {
      if (!projectId || !worktreeId) return null;
      const entries = levels[levelKey(projectId, worktreeId, dir.replace(/\/$/, ""))]?.data?.entries;
      return entries ? entries.map((entry) => ({ name: entry.name, kind: entry.kind })) : null;
    },
    [projectId, worktreeId, levels],
  );
  let state: PathState = "idle";
  if (projectId && path !== null) {
    const level = worktreeId ? levels[levelKey(projectId, worktreeId, path)] : undefined;
    if (level?.data) state = "idle";
    else if (level?.error || treesFailed) state = "failed";
    else state = "loading";
  }
  return { listDir, state };
}
