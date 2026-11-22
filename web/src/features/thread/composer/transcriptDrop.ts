/**
 * A transcript file dragged onto the composer (parity B43). The file is made by the export (`GET /api/threads/{id}/export`) and
 * attached by `POST /api/threads/{id}/transcripts` (`transcriptsClient.ts`; docs/attachments-transcripts.md, "Attach to the
 * composer"): a dropped transcript becomes a chip whose attachment rides on the thread's next message, and every other file goes
 * to the attachment strip. Tests and the dev-only preview install a client with `setTranscriptsApi`; `null` turns the drop off, so
 * every file is then an ordinary attachment. The mock lives in web/mock/attachments.ts.
 */

import { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE } from "../../../lib/transcript";
import type { TranscriptsApi } from "./transcriptContract";
import { realTranscriptsApi } from "./transcriptsClient";

export { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE };
export type { ImportedTranscript, TranscriptsApi } from "./transcriptContract";

/** The service refuses a file over this (`413 file_too_large`); the drop refuses it before sending. */
export const MAX_TRANSCRIPT_BYTES = 2 * 1024 * 1024;

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

/** The production default is the real attach route; tests install `null` to exercise the path with no client. */
let current: TranscriptsApi | null = realTranscriptsApi;

export const transcriptsApi = (): TranscriptsApi | null => current;

/** Tests and the dev-only preview install a client here; `null` is no client, and every file goes to the attachment strip. */
export function setTranscriptsApi(next: TranscriptsApi | null): void {
  current = next;
}

if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/attachments")) {
  void import("../../../../mock/attachments").then((module) => setTranscriptsApi(module.createTranscriptsMock({ delayMs: 800 })));
}
