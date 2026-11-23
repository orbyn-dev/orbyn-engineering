import { ApiFailure, NETWORK_ERROR, authHeaders, failureFrom } from "../../../api/client";
import { serviceUrl } from "../../../api/origin";
import type { ConvertedPhoto as ConvertedPhotoWire } from "../../../generated/ConvertedPhoto";
import type { ConvertedPhoto, PhotoConversionApi } from "./photoConversionContract";

const q = encodeURIComponent;

/**
 * The real photo conversion route (docs/attachments-transcripts.md, "Photo conversion (B8)"). It is Local class, so it
 * carries the same session header as `uploadAttachment` and no person key. The photo is the raw body and its name is the
 * label, encoded in the query.
 */
export const realPhotoConversionApi: PhotoConversionApi = {
  async convert(threadId: string, file: File, signal: AbortSignal): Promise<ConvertedPhoto> {
    let response: Response;
    try {
      response = await fetch(serviceUrl(`/api/threads/${q(threadId)}/attachments/convert?name=${q(file.name)}`), {
        method: "POST",
        credentials: "same-origin",
        signal,
        headers: { ...authHeaders(), "Content-Type": file.type || "application/octet-stream" },
        body: file,
      });
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") throw error;
      throw new ApiFailure(NETWORK_ERROR, "Can't reach the Orbyn service.", 0);
    }
    if (!response.ok) throw await failureFrom(response);
    const wire = (await response.json()) as ConvertedPhotoWire;
    // The generated type says bigint; JSON.parse delivers a number (api/types.ts), so read that number as one.
    return { ...wire, size: Number(wire.size), original_size: Number(wire.original_size) };
  },
};
