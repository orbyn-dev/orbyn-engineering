import { create } from "zustand";

/**
 * The threads that have a terminal waiting for the person (docs/cli-agent-attention.md). The attention store of the terminal panel
 * writes it; the sidebar reads it, so neither imports the other's feature.
 */
const useWaiting = create<{ ids: ReadonlySet<string> }>(() => ({ ids: new Set() }));

export function setWaitingThreads(ids: ReadonlySet<string>): void {
  useWaiting.setState({ ids });
}

/** True while a terminal of the thread waits and has not been dismissed on this window. */
export const useThreadWaiting = (threadId: string): boolean => useWaiting((state) => state.ids.has(threadId));
