/**
 * The photo conversion route's contract. It sits apart from `photoConversion.ts` so `photoConversionClient.ts` can name the
 * types without importing the module that installs it: the arch checker counts type-only imports, and that import would be a
 * cycle.
 */

import type { Wired } from "../../../api/types";
import type { ConvertedPhoto as ConvertedPhotoWire } from "../../../generated/ConvertedPhoto";

/** The service's answer. ts-rs types the sizes as bigint; JSON.parse delivers them as numbers, as api/types.ts notes. */
export type ConvertedPhoto = Wired<ConvertedPhotoWire>;

export type ConversionKind = "convert" | "shrink";

export interface PhotoConversionApi {
  /**
   * Uploads the photo and has the service convert or shrink it. Rejects with `unreadable_image`, `still_too_large`,
   * `file_too_large` or `unsupported_type`, as the route answers them.
   */
  convert(threadId: string, file: File, signal: AbortSignal): Promise<ConvertedPhoto>;
}
