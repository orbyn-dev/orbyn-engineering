/** A mounted thread timeline offers this: bring the row holding a run into view; true when the list has that row. */
export type TimelineScroll = (runId: string) => boolean;

const timelines = new Map<string, TimelineScroll>();

/**
 * Registers the mounted timeline of one thread so "Open the run" can ask it to scroll to a row that is outside the
 * window. A thread has one timeline; the cleanup only removes its own entry, so a late unmount never drops the next one.
 */
export function registerTimelineScroll(threadId: string, scrollToRun: TimelineScroll): () => void {
  timelines.set(threadId, scrollToRun);
  return () => {
    if (timelines.get(threadId) === scrollToRun) timelines.delete(threadId);
  };
}

/** Asks the thread's timeline to scroll to a run's row. False when no timeline is mounted or the list has no such row. */
export const scrollTimelineToRun = (threadId: string, runId: string): boolean => timelines.get(threadId)?.(runId) ?? false;
