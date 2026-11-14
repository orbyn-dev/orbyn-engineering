import { create } from "zustand";
import { validTaskId } from "../writes/api";

interface BoardFocus {
  /** The crew and the task a map node asked the Board to show: the Board reads it once, then clears it. */
  crew: string | null;
  task: string | null;
  /** Asks for a task's card. A task id is service data: one the shared id check refuses is ignored, and never reaches the DOM or a route. */
  ask(crew: string, task: string): boolean;
  clear(): void;
}

export const useBoardFocus = create<BoardFocus>((set) => ({
  crew: null,
  task: null,
  ask: (crew, task) => {
    if (!validTaskId(task) || crew === "") return false;
    set({ crew, task });
    return true;
  },
  clear: () => set({ crew: null, task: null }),
}));
