/**
 * MOCKS of the photo-conversion route and the composer's transcript attach route of docs/attachments-transcripts.md (provisional), for tests and the dev-only
 * preview (`#/preview/attachments`), which installs them through `setPhotoConversionApi` and `setTranscriptsApi`. The app's own
 * defaults are the real clients; nothing here ships. The mocks refuse what the doc says the service refuses, picked by a word in the
 * file name so a person (or a test) can reach every frame: "damaged" and "huge" for a photo; "newer" and "notes" for a transcript.
 * Titles and counts are made up from the name.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { ConvertedPhoto, PhotoConversionApi } from "../src/features/thread/composer/photoConversion.ts";
import type { ImportedTranscript, TranscriptsApi } from "../src/features/thread/composer/transcriptDrop.ts";

const later = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      reject(new DOMException("Aborted", "AbortError"));
    });
  });

export function createPhotoConversionMock(init: { delayMs?: number } = {}): PhotoConversionApi & { sent: string[] } {
  const sent: string[] = [];
  return {
    sent,
    async convert(_threadId, file, signal): Promise<ConvertedPhoto> {
      sent.push(file.name);
      await later(init.delayMs ?? 0, signal);
      if (/damaged/i.test(file.name)) throw new ApiFailure("unreadable_image", "This image could not be read.", 422);
      if (/huge/i.test(file.name)) throw new ApiFailure("still_too_large", "Still over 10 MiB after shrinking.", 422);
      const heic = /\.hei[cf]$/i.test(file.name) || /^image\/hei[cf]$/i.test(file.type);
      const name = heic ? file.name.replace(/\.hei[cf]$/i, ".jpg") : file.name;
      return {
        id: `mock-photo-${sent.length}`,
        name,
        size: heic ? Math.round(file.size / 4) : Math.round(file.size / 3),
        original_name: file.name,
        original_size: file.size,
        action: heic ? "converted" : "shrunk",
      };
    },
  };
}

export function createTranscriptsMock(init: { delayMs?: number } = {}): TranscriptsApi & { sent: string[] } {
  const sent: string[] = [];
  return {
    sent,
    async read(_threadId, file, signal): Promise<ImportedTranscript> {
      sent.push(file.name);
      await later(init.delayMs ?? 0, signal);
      if (/newer/i.test(file.name)) throw new ApiFailure("transcript_too_new", "Made by a newer Orbyn.", 422);
      if (/notes/i.test(file.name)) throw new ApiFailure("transcript_unreadable", "Not a transcript.", 422);
      const title = file.name.replace(/\.orbyn\.json$/i, "").replace(/[-_]+/g, " ");
      return { id: `mock-transcript-${sent.length}`, title: title.charAt(0).toUpperCase() + title.slice(1), message_count: 20 + sent.length * 17 };
    },
  };
}
