import { ApiFailure, NETWORK_ERROR, authHeaders } from "../../../api/client";
import { personKeyHeaders } from "../../../api/personKey";
import { serviceUrl } from "../../../api/origin";
import type { TranscriptAttachAnswer } from "../../../generated/TranscriptAttachAnswer";
import { TRANSCRIPT_TYPE } from "../../../lib/transcript";
import type { ImportedTranscript, TranscriptsApi } from "./transcriptContract";

/** The label `attach_transcript` gives the stored attachment. */
const TITLE_PREFIX = "Imported transcript - ";
const TITLE_SUFFIX = ".txt";

/** The service's label without its heading, or the file's own name when the label is not the service's. */
function importedTitle(name: string, file: File): string {
  if (name.startsWith(TITLE_PREFIX) && name.endsWith(TITLE_SUFFIX)) {
    const title = name.slice(TITLE_PREFIX.length, -TITLE_SUFFIX.length);
    if (title !== "") return title;
  }
  return file.name.replace(/\.orbyn\.json$/i, "");
}

/**
 * The real attach route (docs/attachments-transcripts.md, "Attach to the composer"). The file is untrusted and becomes an
 * attachment of the thread, so the request carries the person key. It is never a dry run: the drop wants the attachment.
 */
export const realTranscriptsApi: TranscriptsApi = {
  async read(threadId: string, file: File, signal: AbortSignal): Promise<ImportedTranscript> {
    const person = await personKeyHeaders("POST", "/api/threads/{id}/transcripts");
    let response: Response;
    try {
      response = await fetch(serviceUrl(`/api/threads/${encodeURIComponent(threadId)}/transcripts`), {
        method: "POST",
        credentials: "same-origin",
        signal,
        headers: { ...authHeaders(), ...person, "Content-Type": TRANSCRIPT_TYPE },
        body: file,
      });
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") throw error;
      throw new ApiFailure(NETWORK_ERROR, "Can't reach the Orbyn service.", 0);
    }
    if (!response.ok) {
      let code = `http_${response.status}`;
      let message = `The server answered ${response.status}.`;
      try {
        const body: unknown = await response.json();
        if (typeof body === "object" && body !== null) {
          const record = body as Record<string, unknown>;
          if (typeof record.code === "string") code = record.code;
          if (typeof record.message === "string") message = record.message;
        }
      } catch {
        // No error body; the status is all there is.
      }
      throw new ApiFailure(code, message, response.status);
    }
    const answer = (await response.json()) as TranscriptAttachAnswer;
    if (!answer.attachment) throw new ApiFailure("transcript_unreadable", "This isn’t a transcript Orbyn can read.", 422);
    return { id: answer.attachment.id, title: importedTitle(answer.attachment.name, file), message_count: answer.message_count };
  },
};
