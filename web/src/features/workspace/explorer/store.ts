import { create } from "zustand";
import { ApiFailure } from "../../../api/client";
import { asBackground } from "../../../api/lanes";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";
import { toFailure } from "../../../lib/useAsync";
import { readTree, readWorktrees } from "./api";

/** One read the explorer keeps: its last good data, whether it is on its way, and why it last failed. */
export interface Kept<T> {
  data: T | null;
  loading: boolean;
  error: ApiFailure | null;
}

/** A folder shows its first 200 children and a row to show the next 200. */
export const PAGE = 200;

export const levelKey = (projectId: string, worktreeId: string, path: string) => JSON.stringify([projectId, worktreeId, path]);

interface Loads {
  /** The open thread, which makes another thread's worktrees read-only for it. */
  thread: string | null;
  quiet?: boolean;
  refresh?: boolean;
}

interface ExplorerStore {
  worktrees: Record<string, Kept<ExplorerWorktrees>>;
  levels: Record<string, Kept<ExplorerTree>>;
  /** Children shown of a folder; more are held than shown once it passes `PAGE`. */
  shown: Record<string, number>;
  loadWorktrees(projectId: string, loads: Loads): Promise<void>;
  loadLevel(projectId: string, worktreeId: string, path: string, loads?: Pick<Loads, "quiet" | "refresh">): Promise<void>;
  showMore(key: string): void;
  /** Reads again what a project has loaded: an event named it (`quiet`), or the person pressed Refresh. */
  reload(projectId: string, loads: Loads): Promise<void>;
  reset(): void;
}

/** Reads running now, by key, so a row asking twice reads once. */
const running = new Map<string, Promise<void>>();
/** The newest read of each key owns its answer. */
const generation = new Map<string, number>();

async function keep<T>(
  key: string,
  read: () => Promise<T>,
  current: () => Kept<T> | undefined,
  put: (next: Kept<T>) => void,
  quiet: boolean,
): Promise<void> {
  const held = running.get(key);
  if (held) return held;
  const id = (generation.get(key) ?? 0) + 1;
  generation.set(key, id);
  const before = current();
  // A quiet read (an event) shows nothing while it runs, and never replaces data with an error.
  if (!quiet) put({ data: before?.data ?? null, loading: true, error: null });
  const started = (quiet ? asBackground(read) : read()).then(
    (data) => {
      if (generation.get(key) === id) put({ data, loading: false, error: null });
    },
    (error: unknown) => {
      if (generation.get(key) !== id) return;
      const held = current()?.data ?? null;
      if (quiet && held) put({ data: held, loading: false, error: null });
      else put({ data: held, loading: false, error: toFailure(error) });
    },
  ).finally(() => running.delete(key));
  running.set(key, started);
  return started;
}

export const useExplorer = create<ExplorerStore>((set, get) => ({
  worktrees: {},
  levels: {},
  shown: {},
  loadWorktrees: (projectId, { thread, quiet = false, refresh = false }) =>
    keep(
      `w:${projectId}`,
      () => readWorktrees(projectId, { thread, refresh }),
      () => get().worktrees[projectId],
      (next) => set((state) => ({ worktrees: { ...state.worktrees, [projectId]: next } })),
      quiet,
    ),
  loadLevel: (projectId, worktreeId, path, { quiet = false, refresh = false } = {}) => {
    const key = levelKey(projectId, worktreeId, path);
    return keep(
      `l:${key}`,
      () => readTree(projectId, worktreeId, path, { refresh }),
      () => get().levels[key],
      (next) => set((state) => ({ levels: { ...state.levels, [key]: next } })),
      quiet,
    );
  },
  showMore: (key) => set((state) => ({ shown: { ...state.shown, [key]: (state.shown[key] ?? PAGE) + PAGE } })),
  async reload(projectId, loads) {
    const { worktrees, levels } = get();
    const keys = Object.keys(levels).filter((key) => (JSON.parse(key) as string[])[0] === projectId);
    await Promise.all([
      worktrees[projectId] ? get().loadWorktrees(projectId, loads) : Promise.resolve(),
      ...keys.map((key) => {
        const [, worktreeId, path] = JSON.parse(key) as [string, string, string];
        return get().loadLevel(projectId, worktreeId, path, loads);
      }),
    ]);
  },
  reset() {
    running.clear();
    generation.clear();
    set({ worktrees: {}, levels: {}, shown: {} });
  },
}));
