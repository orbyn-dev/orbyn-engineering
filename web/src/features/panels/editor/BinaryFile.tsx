import { FileText } from "lucide-react";
import { formatBytes } from "../../../lib/format";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { api } from "../api";
import { EditorEmpty } from "./EditorShell";

/** editor-binary: no text editor for a file that is not text. It says so, gives the size and can reveal the file. Images, PDFs, archives and documents never get here. */
export function BinaryFile({ threadId, path, size }: { threadId: string; path: string; size: number }) {
  const editors = useAsync("editors", (signal) => api.editors(signal));
  const manager = (editors.data ?? []).find((one) => one.file_manager);
  const name = path.split("/").at(-1) ?? path;
  const reveal = async () => {
    if (!manager) return;
    try {
      await api.openInEditor(threadId, { path, editor: manager.id });
    } catch (failure) {
      useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
    }
  };
  return (
    <EditorEmpty
      path={path}
      icon={<FileText size={22} aria-hidden />}
      title="Can’t show this file"
      actions={
        manager ? (
          <Button tone="quiet" onClick={() => void reveal()}>
            Reveal in {manager.name}
          </Button>
        ) : null
      }
    >
      {name} is a binary file ({formatBytes(size)}). Images, PDFs and archives preview in the file tab; this type has no preview.
    </EditorEmpty>
  );
}
