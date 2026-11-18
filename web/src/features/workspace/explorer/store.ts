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

/** Reads running now, by what they ask (slot, thread, refresh), so a row asking twice reads once and a different question is not answered by it. */
interface Flight {
  promise: Promise<void>;
  generation: number;
  again: boolean;
  accepting: boolean;
}
const running = new Map<string, Flight>();
/** Reset fences old completions even if a new request reuses the same slot generation. */
let epoch = 0;
/** The newest read of each slot owns its answer, whatever it asked. */
const generation = new Map<string, number>();

async function keep<T>(
  slot: string,
  asked: string,
  read: () => Promise<T>,
  current: () => Kept<T> | undefined,
  put: (next: Kept<T>) => void,
  quiet: boolean,
): Promise<void> {
  const held = running.get(asked);
  if (held?.accepting && generation.get(slot) === held.generation) {
    // A pushed change is newer than the response already in flight. One rerun
    // absorbs the whole burst without queuing a callback for every event.
    if (quiet) held.again = true;
    return held.promise;
  }
  const id = (generation.get(slot) ?? 0) + 1;
  generation.set(slot, id);
  const startedIn = epoch;
  const owns = () => epoch === startedIn && generation.get(slot) === id;
  const flight: Flight = { promise: Promise.resolve(), generation: id, again: false, accepting: true };
  const before = current();
  if (!quiet) put({ data: before?.data ?? null, loading: true, error: null });
  const started = (async () => {
    try {
      let background = quiet;
      do {
        flight.again = false;
        try {
          const data = await (background ? asBackground(read) : read());
          if (owns()) put({ data, loading: false, error: null });
        } catch (error: unknown) {
          if (!owns()) return;
          const kept = current()?.data ?? null;
          if (background && kept) put({ data: kept, loading: false, error: null });
          else put({ data: kept, loading: false, error: toFailure(error) });
        }
        background = true;
      } while (flight.again && owns());
    } finally {
      // Close admission before this async body completes: an event in the
      // promise-finalizer microtask gap must start a fresh request.
      flight.accepting = false;
    }
  })().finally(() => {
    // A reset may already have started another request with this same key.
    if (running.get(asked) === flight) running.delete(asked);
  });
  flight.promise = started;
  running.set(asked, flight);
  return started;
}

export const useExplorer = create<ExplorerStore>((set, get) => ({
  worktrees: {},
  levels: {},
  shown: {},
  loadWorktrees: (projectId, { thread, quiet = false, refresh = false }) =>
    keep(
      `w:${projectId}`,
      `w:${projectId}:${thread ?? ""}:${refresh}`,
      () => readWorktrees(projectId, { thread, refresh }),
      () => get().worktrees[projectId],
      (next) => set((state) => ({ worktrees: { ...state.worktrees, [projectId]: next } })),
      quiet,
    ),
  loadLevel: (projectId, worktreeId, path, { quiet = false, refresh = false } = {}) => {
    const key = levelKey(projectId, worktreeId, path);
    return keep(
      `l:${key}`,
      `l:${key}:${refresh}`,
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
    epoch += 1;
    running.clear();
    generation.clear();
    set({ worktrees: {}, levels: {}, shown: {} });
  },
}));
