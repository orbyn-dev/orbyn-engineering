import type { Thread } from "../../api/types";

type ExportableThread = Pick<Thread, "id" | "title">;
type Runner = (thread: ExportableThread) => void;

let runner: Runner | null = null;

/** The thread feature offers its export action; the sidebar calls it without importing that feature. */
export function setTranscriptExportRunner(next: Runner | null): void {
  runner = next;
}

/** Exports one thread's transcript, as the sidebar row menu asks. */
export function exportThreadTranscript(thread: ExportableThread): void {
  runner?.(thread);
}
