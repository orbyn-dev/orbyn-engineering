import { useState, type DragEvent } from "react";
import { FILE_MENTION_TYPE } from "../../../product/composer/skillMention";
import { TranscriptShelf } from "./TranscriptShelf";
import { transcriptsApi, willBeTranscript } from "./transcriptDrop";
import { useTranscripts } from "./useTranscripts";

/**
 * Files dragged over and dropped on the composer box: a file mention from the file tree, transcripts (B43, hidden: none without a
 * client) and every other file, which goes to `upload`. `boxProps` goes on the box, `layer` inside it.
 */
export function useComposerDrop(thread: { id: string }, upload: (files: File[]) => void, mention: (name: string) => void) {
  const transcripts = useTranscripts(thread);
  const [dragging, setDragging] = useState(false);
  /** What a drop will do, said over the box. Only a build with a transcript client says it. */
  const [line, setLine] = useState<string | null>(null);
  const end = () => {
    setDragging(false);
    setLine(null);
  };
  return {
    transcripts,
    boxProps: {
      "data-dragging": dragging || undefined,
      onDragOver(event: DragEvent) {
        event.preventDefault();
        setDragging(true);
        if (transcriptsApi() && Array.from(event.dataTransfer.types).includes("Files")) {
          setLine(willBeTranscript(event.dataTransfer.items) ? "Drop to add this thread’s transcript" : "Drop to add the file");
        }
      },
      onDragLeave: end,
      onDrop(event: DragEvent) {
        event.preventDefault();
        end();
        const mentioned = event.dataTransfer.getData(FILE_MENTION_TYPE);
        if (mentioned) return mention(mentioned);
        if (event.dataTransfer.files.length === 0) return;
        const rest = transcripts.take([...event.dataTransfer.files]);
        if (rest.length > 0) upload(rest);
      },
    },
    layer: (
      <>
        <TranscriptShelf
          entries={transcripts.entries}
          remove={transcripts.remove}
          addAsFile={(entry) => {
            upload([entry.file]);
            transcripts.remove(entry.key);
          }}
        />
        {line ? (
          <div className="composer-drop" role="status">
            {line}
          </div>
        ) : null}
      </>
    ),
  };
}
