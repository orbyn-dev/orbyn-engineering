/**
 * The transcript drop's client contract. It sits apart from `transcriptDrop.ts` so `transcriptsClient.ts` can name the types
 * without importing the module that installs it: the arch checker counts type-only imports, and that import would be a cycle.
 */

/** A transcript the service read and kept for this thread. Its id goes in the run's `attachment_ids`, like a file's. */
export interface ImportedTranscript {
  id: string;
  title: string;
  message_count: number;
}

export interface TranscriptsApi {
  /** Uploads the file for the service to read. Rejects with `transcript_too_new`, `transcript_unreadable`, `file_too_large` or `person_required`. */
  read(threadId: string, file: File, signal: AbortSignal): Promise<ImportedTranscript>;
}
