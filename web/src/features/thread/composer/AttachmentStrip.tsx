import { RotateCcw, X } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";

/** A file on its way to the service: `id` once kept, `failure` if it was refused. */
export interface ShownAttachment {
  key: string;
  file: File;
  name: string;
  /** An object URL for images added here, so the shelf can show them. */
  preview: string | null;
  id: string | null;
  failure: string | null;
}

export function AttachmentStrip({
  attachments,
  setAttachments,
  startUpload,
}: {
  attachments: ShownAttachment[];
  setAttachments: Dispatch<SetStateAction<ShownAttachment[]>>;
  startUpload(entry: ShownAttachment): void;
}) {
  return (
    <ul className="attachment-strip" aria-label="Attachments">
      {attachments.map((file) => (
        <li key={file.key} data-failed={file.failure !== null || undefined}>
          {file.preview ? <img src={file.preview} alt="" className="attachment-thumb" /> : null}
          <span title={file.failure ?? undefined}>{file.name}</span>
          {file.failure ? (
            <>
              <span className="attachment-failure" role="alert">
                {file.failure}
              </span>
              <button type="button" aria-label={`Retry ${file.name}`} onClick={() => startUpload(file)}>
                <RotateCcw size={12} aria-hidden />
              </button>
            </>
          ) : file.id === null ? (
            <span className="sr-only" role="status">
              Uploading
            </span>
          ) : null}
          <button
            type="button"
            aria-label={`Remove ${file.name}`}
            onClick={() => setAttachments(attachments.filter((entry) => entry.key !== file.key))}
          >
            <X size={12} aria-hidden />
          </button>
        </li>
      ))}
    </ul>
  );
}
