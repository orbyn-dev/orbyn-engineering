import { useEffect, useRef } from "react";
import type { Project } from "../../../api/types";
import { openTranscriptImport, pickTranscript, setTranscriptPicker, useTranscriptImport } from "../../../product/state/transcriptImport";
import { ImportTranscriptDialog } from "./ImportTranscriptDialog";
import "./exportAction";

/**
 * Holds the one hidden transcript picker and the sheet it opens. Mounted once by the shell, so the folder menu in the sidebar
 * can ask for the picker without importing the thread feature, and a project row can hand it a dropped file.
 */
export function TranscriptImportHost() {
  const request = useTranscriptImport((state) => state.request);
  const close = useTranscriptImport((state) => state.close);
  const input = useRef<HTMLInputElement>(null);
  const target = useRef<Project | null>(null);

  useEffect(() => {
    setTranscriptPicker((project) => {
      target.current = project;
      input.current?.click();
    });
    return () => setTranscriptPicker(null);
  }, []);

  return (
    <>
      <input
        ref={input}
        type="file"
        accept=".orbyn.json,application/json"
        hidden
        data-transcript-file
        onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          const project = target.current;
          event.currentTarget.value = "";
          if (file && project) openTranscriptImport(project, file);
        }}
      />
      {request ? (
        <ImportTranscriptDialog
          key={request.nonce}
          project={request.project}
          file={request.file}
          onClose={close}
          onChooseAnother={() => pickTranscript(request.project)}
        />
      ) : null}
    </>
  );
}
