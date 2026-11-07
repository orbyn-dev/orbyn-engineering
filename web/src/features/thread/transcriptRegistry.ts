import type { TimelineEntry } from "./timeline";

/** What find, shortcuts and pins need from a mounted transcript, since most rows aren't in the DOM. */
export interface TranscriptHandle {
  entries(): readonly TimelineEntry[];
  root(): HTMLElement | null;
  /** Brings a row into the DOM and into view; resolves with its element once mounted. */
  reveal(index: number, align: "start" | "center"): Promise<HTMLElement | null>;
}

const handles = new Map<string, TranscriptHandle>();

export function registerTranscript(threadId: string, handle: TranscriptHandle): () => void {
  handles.set(threadId, handle);
  return () => {
    if (handles.get(threadId) === handle) handles.delete(threadId);
  };
}

export const transcriptFor = (threadId: string): TranscriptHandle | undefined => handles.get(threadId);
