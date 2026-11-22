import { saveBlob } from "../../../lib/download";
import { useUi } from "../../../product/state/ui";
import { setTranscriptExportRunner } from "../../../product/state/transcriptExport";
import { exportBlob, exportFileName, exportTranscript } from "./api";

interface ExportableThread {
  id: string;
  title: string;
}

const EXPORTING = "Exporting transcript…";
const STARTED = "Download started";
const FAILED = "Couldn’t export the transcript.";

/**
 * Exports one thread without a dialog: the menu closes, a busy toast says the file is being made, the service's answer
 * is handed to the browser under the thread's title, and the toast says the download started. A failure offers Try again
 * and never shows the service's words or any message text.
 */
export function startTranscriptExport(thread: ExportableThread): void {
  const ui = useUi.getState();
  const busy = ui.pushToast(EXPORTING, undefined, undefined, { busy: true });
  void (async () => {
    try {
      const document = await exportTranscript(thread.id);
      ui.dismissToast(busy);
      saveBlob(exportFileName(thread.title), exportBlob(document));
      ui.pushToast(STARTED, "done");
    } catch {
      ui.dismissToast(busy);
      ui.pushToast(FAILED, "failed", { label: "Try again", run: () => startTranscriptExport(thread) });
    }
  })();
}

// The sidebar row menu reaches the same action through the product registry.
setTranscriptExportRunner(startTranscriptExport);
