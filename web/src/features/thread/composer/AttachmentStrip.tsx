import { Image as ImageIcon, RotateCcw, X } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import type { ConversionKind } from "./photoConversion";
import { SnapshotDraftChip, useSnapshot, type SnapShotSource } from "../../snapshot";

/** A photo the service is converting or shrinking (parity B8, hidden until core does it): what it is doing and the line to say so. */
export interface PhotoConversion {
  kind: ConversionKind;
  phase: "converting" | "converted" | "failed";
  /** The one line under the name. Empty when failed: the reason is the failure. */
  note: string;
}

/** A file on its way to the service: `id` once kept, `failure` if it was refused. */
export interface ShownAttachment {
  key: string;
  file: File;
  name: string;
  /** An object URL for images added here, so the shelf can show them. */
  preview: string | null;
  id: string | null;
  failure: string | null;
  conversion?: PhotoConversion;
  /** A Snap Shot draft: where it came from, and the shell's id for a discard when the person removes it. */
  snapshot?: { source: SnapShotSource; captureId: string };
}

function PhotoChip({
  file,
  retry,
  cancel,
  remove,
}: {
  file: ShownAttachment & { conversion: PhotoConversion };
  retry(): void;
  cancel(): void;
  remove(): void;
}) {
  const { phase, kind } = file.conversion;
  return (
    <li data-photo="" data-conversion={phase} data-failed={phase === "failed" || undefined}>
      <span className="photo-box">
        {phase === "converting" ? (
          <i className="spin photo-spin" aria-label="Working" />
        ) : file.preview ? (
          <img src={file.preview} alt="" className="attachment-thumb" />
        ) : (
          <ImageIcon size={14} aria-hidden />
        )}
      </span>
      <span className="photo-text">
        <span className="photo-name">{file.name}</span>
        {phase === "failed" ? (
          <span className="photo-line photo-failure" role="alert">
            {file.failure}
          </span>
        ) : (
          <span className="photo-line" role={phase === "converting" ? "status" : undefined}>
            {file.conversion.note}
          </span>
        )}
      </span>
      {phase === "failed" ? (
        <button type="button" className="photo-retry" aria-label={`Retry ${file.name}`} onClick={retry}>
          <RotateCcw size={12} aria-hidden />
        </button>
      ) : null}
      {phase === "converting" ? (
        <button type="button" aria-label={`Cancel ${kind === "convert" ? "converting" : "shrinking"} ${file.name}`} onClick={cancel}>
          <X size={12} aria-hidden />
        </button>
      ) : (
        <button type="button" aria-label={`Remove ${file.name}`} onClick={remove}>
          <X size={12} aria-hidden />
        </button>
      )}
    </li>
  );
}

export function AttachmentStrip({
  attachments,
  setAttachments,
  startUpload,
  cancel,
  onRemoveSnapshotText,
}: {
  attachments: ShownAttachment[];
  setAttachments: Dispatch<SetStateAction<ShownAttachment[]>>;
  startUpload(entry: ShownAttachment): void;
  /** Ends a conversion in flight and drops its chip. Without it a converting chip can only be removed. */
  cancel?(entry: ShownAttachment): void;
  /** Drops the accessible text from a capture, on the service when it is already kept. */
  onRemoveSnapshotText?(entry: ShownAttachment): void;
}) {
  const remove = (key: string) => setAttachments(attachments.filter((entry) => entry.key !== key));
  return (
    <ul className="attachment-strip" aria-label="Attachments">
      {attachments.map((file) =>
        file.snapshot ? (
          <li key={file.key} className="attachment-snapshot">
            <SnapshotDraftChip
              name={file.name}
              preview={file.preview}
              source={file.snapshot.source}
              onRemove={() => {
                void useSnapshot.getState().discard(file.snapshot!.captureId);
                if (file.preview) URL.revokeObjectURL(file.preview);
                remove(file.key);
              }}
              onRemoveText={() => onRemoveSnapshotText?.(file)}
            />
          </li>
        ) : file.conversion ? (
          <PhotoChip
            key={file.key}
            file={{ ...file, conversion: file.conversion }}
            retry={() => startUpload(file)}
            cancel={() => (cancel ? cancel(file) : remove(file.key))}
            remove={() => remove(file.key)}
          />
        ) : (
          <li key={file.key} data-failed={file.failure !== null || undefined} data-photo={file.preview ? "" : undefined}>
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
            <button type="button" aria-label={`Remove ${file.name}`} onClick={() => remove(file.key)}>
              <X size={12} aria-hidden />
            </button>
          </li>
        ),
      )}
    </ul>
  );
}
