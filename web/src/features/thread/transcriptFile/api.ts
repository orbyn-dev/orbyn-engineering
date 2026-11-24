import { ApiFailure, NETWORK_ERROR, authHeaders, request } from "../../../api/client";
import { personKeyHeaders } from "../../../api/personKey";
import { serviceUrl } from "../../../api/origin";
import type { PreferenceView } from "../../../api/types";
import type { TranscriptDocument } from "../../../generated/TranscriptDocument";
import type { TranscriptImportAnswer } from "../../../generated/TranscriptImportAnswer";
import { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE } from "../composer/transcriptDrop";

/** The service refuses a file over this (`413 file_too_large`); the page stops it before sending. */
export const MAX_IMPORT_BYTES = 2 * 1024 * 1024;

/** The project's model defaults and the engines it allows, as the import sheet reads them. */
export const readProjectPreferences = (projectId: string, signal?: AbortSignal): Promise<PreferenceView[]> =>
  request<PreferenceView[]>("GET", `/api/preferences?project_id=${encodeURIComponent(projectId)}`, undefined, signal);

/** The thread as one document, already redacted by the service. Nothing is written on the computer here. */
export const exportTranscript = (threadId: string, signal?: AbortSignal): Promise<TranscriptDocument> =>
  request<TranscriptDocument>("GET", `/api/threads/${encodeURIComponent(threadId)}/export`, undefined, signal);

/** The file name a saved export gets: the title, cleaned for a file system, plus the ending. */
export function exportFileName(title: string): string {
  const printable = Array.from(title, (char) => (char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127 ? " " : char)).join("");
  const clean = printable.replace(/[<>:"/\\|?*]+/g, " ").replace(/\s+/g, " ").trim().replace(/^\.+/, "").slice(0, 80).trim();
  return `${clean || "Thread"}${TRANSCRIPT_EXTENSION}`;
}

/** The document as a file for the browser to save. */
export const exportBlob = (document: TranscriptDocument): Blob => new Blob([JSON.stringify(document, null, 2)], { type: TRANSCRIPT_TYPE });

export interface ImportRequest {
  projectId: string;
  /** Left out, the service uses the engine the file names. */
  engine?: string;
  /** Checks the file and answers counts and problems; creates nothing. */
  dryRun: boolean;
  file: File;
}

/**
 * Person-class: the file is untrusted and creates a thread, so the request carries the person key.
 * Rejects with `file_too_large` (413), `transcript_too_new` or `transcript_unreadable` (422), or `person_required`.
 */
export async function importTranscript({ projectId, engine, dryRun, file }: ImportRequest, signal?: AbortSignal): Promise<TranscriptImportAnswer> {
  if (file.size > MAX_IMPORT_BYTES) throw new ApiFailure("file_too_large", "That file is over 2 MiB.", 413);
  const query = new URLSearchParams({ project_id: projectId, name: file.name.slice(0, 255) });
  if (engine) query.set("engine", engine);
  if (dryRun) query.set("dry_run", "true");
  const path = `/api/threads/import?${query.toString()}`;
  const person = await personKeyHeaders("POST", "/api/threads/import");
  let response: Response;
  try {
    response = await fetch(serviceUrl(path), {
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
  return (await response.json()) as TranscriptImportAnswer;
}
