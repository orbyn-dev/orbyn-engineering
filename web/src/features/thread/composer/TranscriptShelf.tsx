import { MessageSquare, X } from "lucide-react";
import { Button } from "../../../ui/controls";
import type { TranscriptEntry } from "./useTranscripts";

/**
 * Transcripts dropped on the composer (B43). A kept one is a 24 px chip like a thread chip; one being read or refused is
 * the 40 px chip of a photo, in red with its reason when refused. Names and titles come from a file and are drawn as text.
 */
export function TranscriptShelf({
  entries,
  remove,
  addAsFile,
}: {
  entries: readonly TranscriptEntry[];
  remove(key: string): void;
  addAsFile(entry: TranscriptEntry): void;
}) {
  if (entries.length === 0) return null;
  const kept = entries.filter((entry) => entry.state === "ready");
  const others = entries.filter((entry) => entry.state !== "ready");
  return (
    <>
      {kept.length > 0 ? (
        <ul className="context-chips" aria-label="Transcripts">
          {kept.map((entry) =>
            entry.state === "ready" ? (
              <li key={entry.key} data-kind="transcript">
                <MessageSquare size={12} aria-hidden />
                <span title={entry.title}>{entry.title}</span>
                <span className="transcript-count">
                  · {entry.messageCount.toLocaleString()} {entry.messageCount === 1 ? "message" : "messages"}
                </span>
                <button type="button" aria-label={`Remove ${entry.title}`} onClick={() => remove(entry.key)}>
                  <X size={12} aria-hidden />
                </button>
              </li>
            ) : null,
          )}
        </ul>
      ) : null}
      {others.length > 0 ? (
        <ul className="attachment-strip" aria-label="Transcript files">
          {others.map((entry) => (
            <li key={entry.key} data-photo="" data-failed={entry.state === "refused" || undefined}>
              <span className="photo-box">{entry.state === "reading" ? <i className="spin photo-spin" aria-label="Working" /> : <MessageSquare size={14} aria-hidden />}</span>
              <span className="photo-text">
                <span className="photo-name">{entry.file.name}</span>
                {entry.state === "refused" ? (
                  <span className="photo-line photo-failure" role="alert">
                    {entry.reason}
                  </span>
                ) : (
                  <span className="photo-line" role="status">
                    Reading transcript…
                  </span>
                )}
              </span>
              {entry.state === "refused" && entry.canAddAsFile ? (
                <Button small onClick={() => addAsFile(entry)}>
                  Add as a file
                </Button>
              ) : null}
              <button
                type="button"
                aria-label={entry.state === "reading" ? `Cancel reading ${entry.file.name}` : `Remove ${entry.file.name}`}
                onClick={() => remove(entry.key)}
              >
                <X size={12} aria-hidden />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </>
  );
}
