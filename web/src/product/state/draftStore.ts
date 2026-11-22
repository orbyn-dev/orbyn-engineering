import { create } from "zustand";

export const DRAFT_KEY = "new-thread";

/** A Snap Shot held for the thread the person has not made yet. The source stays opaque to the store. */
export interface DraftSnapShot {
  key: string;
  file: File;
  name: string;
  preview: string;
  source: unknown;
  captureId: string;
}
export const SCRATCH = "scratch";

/** What waits for a thread that doesn't exist yet, or for a composer that isn't open. */
interface DraftStore {
  /** Files for the next new thread, so a project switch or the trust step keeps them. */
  files: File[];
  setFiles(files: File[]): void;
  /** Snap Shot drafts for the next new thread, kept like its files. */
  snapShots: DraftSnapShot[];
  setSnapShots(snapShots: DraftSnapShot[]): void;
  /** A worktree the next new thread reuses, by its purpose. */
  reuseWorktree: string | null;
  setReuseWorktree(purpose: string | null): void;
  /** The next new thread works in the project's own checkout, whatever the device default is. Taken once. */
  currentCheckout: boolean;
  setCurrentCheckout(on: boolean): void;
  /** Files dropped on a thread's row, on their way to its composer. */
  dropped: Record<string, File[]>;
  drop(threadId: string, files: File[]): void;
  takeDropped(threadId: string): File[];
}

export const useDraftStore = create<DraftStore>((set, get) => ({
  files: [],
  setFiles: (files) => set({ files }),
  snapShots: [],
  setSnapShots: (snapShots) => set({ snapShots }),
  reuseWorktree: null,
  setReuseWorktree: (reuseWorktree) => set({ reuseWorktree }),
  currentCheckout: false,
  setCurrentCheckout: (currentCheckout) => set({ currentCheckout }),
  dropped: {},
  drop: (threadId, files) =>
    set((state) => ({ dropped: { ...state.dropped, [threadId]: [...(state.dropped[threadId] ?? []), ...files] } })),
  takeDropped(threadId) {
    const { [threadId]: taken = [], ...others } = get().dropped;
    set({ dropped: others });
    return taken;
  },
}));

/** The folder name is the purpose the service made the worktree from. */
export const worktreePurpose = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? "";
