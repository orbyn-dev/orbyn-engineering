/**
 * Photo conversion (parity B8), built hidden. Core does not convert a HEIC photo to JPEG or shrink an oversized one yet, so the
 * web has no client for it: `photoConversionApi()` answers `null` in a production build, the composer then uploads exactly as it
 * does today and sends nothing new. The route it would call is written in docs/attachments-transcripts.md; the mock that draws
 * the frames lives in web/mock/attachments.ts and is installed only by the dev-only preview or by a test.
 */

export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

/** What came back for a photo the service converted or shrank. The file sent is this one, never the one picked. */
export interface ConvertedPhoto {
  id: string;
  name: string;
  size: number;
  original_name: string;
  original_size: number;
  action: "converted" | "shrunk";
}

export interface PhotoConversionApi {
  /** Uploads the photo and has the service convert or shrink it. Rejects with `unreadable_image` or `still_too_large`. */
  convert(threadId: string, file: File, signal: AbortSignal): Promise<ConvertedPhoto>;
}

export type ConversionKind = "convert" | "shrink";

const HEIC = /\.(heic|heif)$/i;
/** Types the service can shrink. A GIF is not one of them: shrinking would drop its frames. */
const SHRINKABLE = new Set(["image/png", "image/jpeg", "image/webp"]);

/** Whether a client would have to convert this photo, shrink it, or leave it alone. Only a hint for the chip. */
export function conversionKindOf(file: File): ConversionKind | null {
  if (/^image\/hei[cf]$/i.test(file.type) || (file.type === "" && HEIC.test(file.name))) return "convert";
  if (SHRINKABLE.has(file.type) && file.size > MAX_IMAGE_BYTES) return "shrink";
  return null;
}

let current: PhotoConversionApi | null = null;

export const photoConversionApi = (): PhotoConversionApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production default, which sends nothing. */
export function setPhotoConversionApi(next: PhotoConversionApi | null): void {
  current = next;
}

// Dev only: `#/preview/attachments` installs the mock. In a production build the flag is false and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/attachments")) {
  void import("../../../../mock/attachments").then((module) => setPhotoConversionApi(module.createPhotoConversionMock({ delayMs: 1200 })));
}
