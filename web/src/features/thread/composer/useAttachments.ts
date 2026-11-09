import { useRef, useState } from "react";
import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import type { ShownAttachment } from "./AttachmentStrip";

/** The files on the shelf, each uploaded on its own. */
export function useAttachments(thread: Thread, setError: (message: string | null) => void) {
  const [attachments, setAttachments] = useState<ShownAttachment[]>([]);
  const filePicker = useRef<HTMLInputElement>(null);
  const uploading = attachments.some((file) => file.id === null);

  const startUpload = (entry: ShownAttachment) => {
    const settle = (patch: Partial<ShownAttachment>) =>
      setAttachments((current) => current.map((file) => (file.key === entry.key ? { ...file, ...patch } : file)));
    settle({ failure: null });
    api.uploadAttachment(thread.id, entry.file).then(
      (kept) => settle({ id: kept.id, name: kept.name }),
      (failure: unknown) => settle({ failure: toFailure(failure).message }),
    );
  };

  /** Each file starts at once and ends up kept or refused on its own. */
  const upload = (files: FileList | File[] | null) => {
    if (!files || files.length === 0) return;
    setError(null);
    const added = [...files].map((file) => ({
      key: crypto.randomUUID(),
      file,
      name: file.name,
      preview: file.type.startsWith("image/") ? URL.createObjectURL(file) : null,
      id: null,
      failure: null,
    }));
    setAttachments((current) => [...current, ...added]);
    added.forEach(startUpload);
    if (filePicker.current) filePicker.current.value = "";
  };

  return { attachments, setAttachments, filePicker, uploading, startUpload, upload };
}
