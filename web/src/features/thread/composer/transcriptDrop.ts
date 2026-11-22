/**
 * A transcript file dragged onto the composer (parity B43). The file is made by the export (`GET /api/threads/{id}/export`). The per-thread attach route this drop would use is not
 * wired here, so `transcriptsApi()` answers `null` in a production build: a file dropped on the composer is then an ordinary attachment, as today. The route is
 * written in docs/attachments-transcripts.md; the mock lives in web/mock/attachments.ts.
 */

import { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE } from "../../../lib/transcript";

export { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE };

export const MAX_TRANSCRIPT_BYTES = 50 * 1024 * 1024;

/** A transcript the service read and kept for this thread. Its id goes in the run's `attachment_ids`, like a file's. */
export interface ImportedTranscript {
  id: string;
  title: string;
  message_count: number;
}

export interface TranscriptsApi {
  /** Uploads the file for the service to read. Rejects with `transcript_too_new`, `transcript_unreadable` or `file_too_large`. */
  read(threadId: string, file: File, signal: AbortSignal): Promise<ImportedTranscript>;
}

export const looksLikeTranscript = (file: File): boolean => file.name.toLowerCase().endsWith(TRANSCRIPT_EXTENSION);

/** One drop, split: transcripts in the order dropped, and every other file for the attachment strip. */
export function splitDrop(files: readonly File[]): { transcripts: File[]; files: File[] } {
  return { transcripts: files.filter(looksLikeTranscript), files: files.filter((file) => !looksLikeTranscript(file)) };
}

/**
 * Whether everything held over the composer is a transcript. A browser hides names until the drop and shows only a type, so
 * this reads the type; when it cannot tell, the box says "Drop to add the file" and the drop decides.
 */
export function willBeTranscript(items: ArrayLike<DataTransferItem>): boolean {
  const held = Array.from(items).filter((item) => item.kind === "file");
  return held.length > 0 && held.every((item) => item.type === TRANSCRIPT_TYPE);
}

let current: TranscriptsApi | null = null;

export const transcriptsApi = (): TranscriptsApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production default, which sends nothing. */
export function setTranscriptsApi(next: TranscriptsApi | null): void {
  current = next;
}

if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/attachments")) {
  void import("../../../../mock/attachments").then((module) => setTranscriptsApi(module.createTranscriptsMock({ delayMs: 800 })));
}
