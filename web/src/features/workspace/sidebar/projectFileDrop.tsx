import { FileText } from "lucide-react";
import { useState, type DragEvent, type HTMLAttributes } from "react";
import { createPortal } from "react-dom";
import type { Project } from "../../../api/types";
import { TRANSCRIPT_EXTENSION, TRANSCRIPT_TYPE } from "../../../lib/transcript";
import { openTranscriptImport } from "../../../product/state/transcriptImport";
import "./projectFileDrop.css";

export interface TranscriptDropHint {
  name: string | null;
  label: string;
  x: number;
  y: number;
}

const jsonType = (type: string) =>
  type === "" || type === "application/json" || type === "text/json" || type === TRANSCRIPT_TYPE || type.endsWith("+json") || type.endsWith("/json");

/** What is held, when a project row can take it: a JSON or unknown file, never a known other kind such as an image. */
function heldTranscript(transfer: DataTransfer | null): { name: string | null; named: boolean } | null {
  if (!transfer) return null;
  const items = transfer.items ? Array.from(transfer.items) : [];
  const files = items.filter((item) => item.kind === "file");
  if (files.length > 0) {
    const file = files[0]?.getAsFile();
    // A transcript's own ending wins over a type the system guessed, because a system may call it text/plain.
    if (file?.name.toLowerCase().endsWith(TRANSCRIPT_EXTENSION)) return { name: file.name, named: true };
    if (!files.every((item) => jsonType(item.type))) return null;
    return { name: file?.name ?? null, named: false };
  }
  // Some systems hide names and types until the drop. A file is then worth offering the row; the sheet refuses one
  // that turns out not to be a transcript.
  return Array.from(transfer.types ?? []).includes("Files") ? { name: null, named: false } : null;
}

const hintFor = (project: Project, held: { name: string | null; named: boolean }): Omit<TranscriptDropHint, "x" | "y"> => ({
  name: held.name,
  label: held.named ? `Drop to import into ${project.name}` : "Drop to import a transcript",
});

/** The dashed outline and the pointer label for a transcript held over a project row; a drop opens the import sheet. */
export function useProjectFileDrop(project: Project) {
  const [hint, setHint] = useState<TranscriptDropHint | null>(null);
  const offer = (event: DragEvent<HTMLElement>) => {
    const held = heldTranscript(event.dataTransfer);
    if (!held) return null;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    return held;
  };
  const handlers: Pick<HTMLAttributes<HTMLElement>, "onDragEnter" | "onDragOver" | "onDragLeave" | "onDrop"> = {
    onDragEnter(event) {
      const held = offer(event);
      if (held) setHint({ ...hintFor(project, held), x: event.clientX + 16, y: event.clientY + 16 });
    },
    onDragOver(event) {
      const held = offer(event);
      if (!held) return;
      setHint(() => ({
        ...hintFor(project, held),
        x: event.clientX + 16,
        y: event.clientY + 16,
      }));
    },
    onDragLeave(event) {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setHint(null);
    },
    onDrop(event) {
      const held = heldTranscript(event.dataTransfer);
      setHint(null);
      if (!held) return;
      event.preventDefault();
      event.stopPropagation();
      const file = event.dataTransfer?.files?.[0];
      if (file) openTranscriptImport(project, file);
    },
  };
  return { hint, over: hint !== null, handlers };
}

/** The small label beside the pointer that says what dropping the held file does. */
export function TranscriptDropHint({ hint }: { hint: TranscriptDropHint }) {
  return createPortal(
    <div className="transcript-drop-hint" role="status" style={{ left: hint.x, top: hint.y }}>
      {hint.name ? (
        <span className="transcript-drop-name">
          <FileText size={14} aria-hidden />
          <span>{hint.name}</span>
        </span>
      ) : null}
      <span className="transcript-drop-label">{hint.label}</span>
    </div>,
    document.body,
  );
}
