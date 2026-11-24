import { create } from "zustand";

/** A crew id is service data: one the shared check refuses never reaches the DOM or a route. */
const validCrewId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

interface WatchFocus {
  /** The watch a Needs you row or another screen asked the Background tab to open: the tab reads it once, then clears it. */
  id: string | null;
  /** The project that watch belongs to, when the asker knows it: the tab opens on that project instead of the first git project. */
  project: string | null;
  /** Asks for one watch's page. A crew or project id the shared check refuses is ignored. */
  ask(id: string, project?: string | null): boolean;
  clear(): void;
}

export const useWatchFocus = create<WatchFocus>((set) => ({
  id: null,
  project: null,
  ask: (id, project) => {
    if (!validCrewId(id)) return false;
    set({ id, project: project && validCrewId(project) ? project : null });
    return true;
  },
  clear: () => set({ id: null, project: null }),
}));
