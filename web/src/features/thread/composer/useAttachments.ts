import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Thread } from "../../../api/types";
import { formatBytes } from "../../../lib/format";
import { toFailure } from "../../../lib/useAsync";
import type { ShownAttachment } from "./AttachmentStrip";
import { conversionKindOf, photoConversionApi, type ConversionKind } from "./photoConversion";

const REFUSALS: Record<string, string> = {
  unreadable_image: "Couldn’t convert this photo. It may be damaged.",
  still_too_large: "Still over 10 MiB after shrinking. Pick a smaller photo.",
};

const workingNote = (kind: ConversionKind, file: File) =>
  kind === "convert" ? "Converting photo…" : `Shrinking photo · ${formatBytes(file.size)}…`;

/** The files on the shelf, each uploaded on its own. */
export function useAttachments(thread: Thread, setError: (message: string | null) => void) {
  const [attachments, setAttachments] = useState<ShownAttachment[]>([]);
  const filePicker = useRef<HTMLInputElement>(null);
  const uploading = attachments.some((file) => file.id === null);
  /** The conversions in flight, so a cancel (or leaving the thread) ends the request. */
  const running = useRef(new Map<string, AbortController>());
  useEffect(() => {
    const live = running.current;
    return () => live.forEach((control) => control.abort());
  }, []);

  const startUpload = (entry: ShownAttachment) => {
    const settle = (patch: Partial<ShownAttachment>) =>
      setAttachments((current) => current.map((file) => (file.key === entry.key ? { ...file, ...patch } : file)));
    const client = photoConversionApi();
    if (entry.conversion && client) {
      const { kind } = entry.conversion;
      const control = new AbortController();
      running.current.set(entry.key, control);
      settle({ failure: null, conversion: { kind, phase: "converting", note: workingNote(kind, entry.file) } });
      client.convert(thread.id, entry.file, control.signal).then(
        (done) => {
          if (control.signal.aborted) return;
          settle({
            id: done.id,
            name: done.name,
            conversion: {
              kind,
              phase: "converted",
              note: done.action === "converted"
                ? `Converted to JPEG · ${formatBytes(done.size)}`
                : `Shrunk from ${formatBytes(done.original_size)} to ${formatBytes(done.size)}`,
            },
          });
        },
        (failure: unknown) => {
          if (control.signal.aborted) return;
          const known = toFailure(failure);
          settle({ failure: REFUSALS[known.code] ?? known.message, conversion: { kind, phase: "failed", note: "" } });
        },
      ).finally(() => running.current.delete(entry.key));
      return;
    }
    settle({ failure: null });
    api.uploadAttachment(thread.id, entry.file).then(
      (kept) => settle({ id: kept.id, name: kept.name }),
      (failure: unknown) => settle({ failure: toFailure(failure).message }),
    );
  };

  /** Ends a conversion and takes its chip off the shelf. */
  const cancel = (entry: ShownAttachment) => {
    running.current.get(entry.key)?.abort();
    running.current.delete(entry.key);
    setAttachments((current) => current.filter((file) => file.key !== entry.key));
  };

  /** Each file starts at once and ends up kept or refused on its own. */
  const upload = (files: FileList | File[] | null) => {
    if (!files || files.length === 0) return;
    setError(null);
    const converts = photoConversionApi() !== null;
    const added: ShownAttachment[] = [...files].map((file) => {
      const kind = converts ? conversionKindOf(file) : null;
      return {
        key: crypto.randomUUID(),
        file,
        name: file.name,
        // A HEIC photo is not a picture a browser can draw, so its chip shows no thumbnail until it is a JPEG.
        preview: file.type.startsWith("image/") && kind !== "convert" ? URL.createObjectURL(file) : null,
        id: null,
        failure: null,
        ...(kind ? { conversion: { kind, phase: "converting" as const, note: workingNote(kind, file) } } : {}),
      };
    });
    setAttachments((current) => [...current, ...added]);
    added.forEach(startUpload);
    if (filePicker.current) filePicker.current.value = "";
  };

  return { attachments, setAttachments, filePicker, uploading, startUpload, upload, cancel };
}
