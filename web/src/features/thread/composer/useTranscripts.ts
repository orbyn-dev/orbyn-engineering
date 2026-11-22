import { useEffect, useRef, useState } from "react";
import { toFailure } from "../../../lib/useAsync";
import { looksLikeTranscript, MAX_TRANSCRIPT_BYTES, transcriptsApi } from "./transcriptDrop";

/** A transcript file dropped on the composer, from being read to being kept or refused. */
export type TranscriptEntry =
  | { key: string; file: File; state: "reading" }
  | { key: string; file: File; state: "ready"; id: string; title: string; messageCount: number }
  | { key: string; file: File; state: "refused"; reason: string; canAddAsFile: boolean };

const DESKTOP_ONLY = "This action is only available in the desktop app.";
const TOO_BIG = "Transcripts can be up to 2 MiB.";
const REASONS: Record<string, { reason: string; canAddAsFile: boolean }> = {
  transcript_too_new: { reason: "Made by a newer Orbyn. Update Orbyn to read it.", canAddAsFile: false },
  transcript_unreadable: { reason: "This isn’t a transcript Orbyn can read.", canAddAsFile: true },
  file_too_large: { reason: TOO_BIG, canAddAsFile: false },
  // The same sentence as the import sheet, for a request that carries no person key.
  desktop_required: { reason: DESKTOP_ONLY, canAddAsFile: false },
  person_required: { reason: DESKTOP_ONLY, canAddAsFile: false },
};

/**
 * The transcripts waiting above one composer (B43). With no client installed `take` keeps nothing and every file goes to the
 * attachment strip, as before.
 */
export function useTranscripts(thread: { id: string }) {
  const [entries, setEntries] = useState<TranscriptEntry[]>([]);
  const running = useRef(new Map<string, AbortController>());
  useEffect(() => {
    const live = running.current;
    return () => live.forEach((control) => control.abort());
  }, []);

  const settle = (key: string, next: TranscriptEntry) =>
    setEntries((current) => current.map((entry) => (entry.key === key ? next : entry)));

  /** Reads the transcripts of a drop in the order dropped and returns every other file, for the attachment strip. */
  const take = (files: readonly File[]): File[] => {
    const client = transcriptsApi();
    if (!client) return [...files];
    const entering: TranscriptEntry[] = files.filter(looksLikeTranscript).map((file) =>
      file.size > MAX_TRANSCRIPT_BYTES
        ? { key: crypto.randomUUID(), file, state: "refused", reason: TOO_BIG, canAddAsFile: false }
        : { key: crypto.randomUUID(), file, state: "reading" },
    );
    if (entering.length > 0) setEntries((current) => [...current, ...entering]);
    for (const entry of entering) {
      if (entry.state !== "reading") continue;
      const control = new AbortController();
      running.current.set(entry.key, control);
      client.read(thread.id, entry.file, control.signal).then(
        (kept) => {
          if (!control.signal.aborted) settle(entry.key, { key: entry.key, file: entry.file, state: "ready", id: kept.id, title: kept.title, messageCount: kept.message_count });
        },
        (failure: unknown) => {
          if (control.signal.aborted) return;
          const known = REASONS[toFailure(failure).code];
          settle(entry.key, {
            key: entry.key,
            file: entry.file,
            state: "refused",
            reason: known?.reason ?? "Couldn’t read this transcript.",
            canAddAsFile: known?.canAddAsFile ?? false,
          });
        },
      ).finally(() => running.current.delete(entry.key));
    }
    return files.filter((file) => !looksLikeTranscript(file));
  };

  const remove = (key: string) => {
    running.current.get(key)?.abort();
    running.current.delete(key);
    setEntries((current) => current.filter((entry) => entry.key !== key));
  };

  return {
    entries,
    take,
    remove,
    /** A send leaves the transcripts that were sent and keeps a refused one, which was not. */
    consume: () => setEntries((current) => current.filter((entry) => entry.state !== "ready")),
    reading: entries.some((entry) => entry.state === "reading"),
    ids: entries.flatMap((entry) => (entry.state === "ready" ? [entry.id] : [])),
  };
}
