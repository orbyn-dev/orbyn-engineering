import type { Dispatch, SetStateAction } from "react";
import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { sourceBodyOf, sourceBodyToSend, useSnapshotDraftFlow, withoutAccessibleText } from "../../snapshot";
import type { ShownAttachment } from "./AttachmentStrip";

/** The composer's Snap Shot wiring: the button and shortcut land here, and a taken PNG is uploaded and sourced at once. */
export function useComposerSnapshot(
  thread: Thread,
  setAttachments: Dispatch<SetStateAction<ShownAttachment[]>>,
  setError: (message: string | null) => void,
) {
  const flow = useSnapshotDraftFlow({
    kind: "thread",
    threadId: thread.id,
    label: thread.title ?? "this thread",
    receive: async (draft) => {
      const kept = await api.uploadAttachment(thread.id, draft.file);
      await api.attachmentSource(kept.id, sourceBodyToSend(draft.source, useUi.getState().snapshotIncludeAccessibility));
      setAttachments((current) => [...current, {
        key: draft.captureId,
        file: draft.file,
        name: kept.name,
        preview: draft.preview,
        id: kept.id,
        failure: null,
        snapshot: { source: draft.source, captureId: draft.captureId },
      }]);
    },
  });
  const removeText = (entry: ShownAttachment) => {
    if (!entry.snapshot || !entry.id) return;
    const source = withoutAccessibleText(entry.snapshot.source);
    void api.attachmentSource(entry.id, sourceBodyOf(source)).catch((failure: unknown) => setError(toFailure(failure).message));
    setAttachments((current) => current.map((file) => file.key === entry.key
      ? { ...file, snapshot: { ...file.snapshot!, source } }
      : file));
  };
  return { flow, removeText };
}
