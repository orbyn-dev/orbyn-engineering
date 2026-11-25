/**
 * Photo conversion (parity B8). The real route is `POST /api/threads/{id}/attachments/convert` (see
 * `photoConversionClient.ts`): the service shrinks a PNG or JPEG over 10 MiB and rewrites one within the limit, decodes a
 * WebP and writes it again, and refuses HEIC or HEIF until a decoder is linked. `photoConversionApi()` answers the production client by default;
 * tests and the dev-only preview install another with `setPhotoConversionApi`, and `setPhotoConversionApi(null)` restores
 * the production client. The mock lives in web/mock/attachments.ts and is installed only by the dev-only preview.
 */

import { realPhotoConversionApi } from "./photoConversionClient";
import type { ConversionKind, PhotoConversionApi } from "./photoConversionContract";

export type { ConversionKind, ConvertedPhoto, PhotoConversionApi } from "./photoConversionContract";

export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

/** Name fallbacks when the system gives no type at all, as a phone's file picker sometimes does. */
const HEIC_NAME = /\.(heic|heif)$/i;
const WEBP_NAME = /\.webp$/i;
/** Types the service writes again smaller. A GIF is not one of them: shrinking would drop its frames. */
const SHRINKABLE: ReadonlySet<string> = new Set(["image/png", "image/jpeg"]);
/**
 * Types that go to the route even when small: WebP so the service writes it again from decoded pixels, and HEIC and
 * HEIF so the service's refusal message is shown instead of a silent upload until a decoder is linked.
 */
const CONVERT: ReadonlySet<string> = new Set(["image/heic", "image/heif", "image/webp"]);

/**
 * Whether this photo should go to the conversion route: HEIC, HEIF and WebP always, or a PNG or JPEG over the limit to
 * shrink. `null` means upload it exactly as before.
 */
export function conversionKindOf(file: File): ConversionKind | null {
  const type = file.type.toLowerCase();
  if (CONVERT.has(type)) return "convert";
  if (type === "" && (HEIC_NAME.test(file.name) || WEBP_NAME.test(file.name))) return "convert";
  if (SHRINKABLE.has(type) && file.size > MAX_IMAGE_BYTES) return "shrink";
  return null;
}

let current: PhotoConversionApi | null = realPhotoConversionApi;

export const photoConversionApi = (): PhotoConversionApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts the production client back. */
export function setPhotoConversionApi(next: PhotoConversionApi | null): void {
  current = next ?? realPhotoConversionApi;
}

// Dev only: `#/preview/attachments` installs the mock. In a production build the flag is false and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/attachments")) {
  void import("../../../../mock/attachments").then((module) => setPhotoConversionApi(module.createPhotoConversionMock({ delayMs: 1200 })));
}
