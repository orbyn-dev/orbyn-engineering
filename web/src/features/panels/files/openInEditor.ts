import { api } from "../api";
import { toFailure } from "../../../lib/useAsync";
import { primaryEditor } from "../../../product/composer/editorPreferences";
import { useUi } from "../../../product/state/ui";

/** Opens a path of the thread's checkout in the person's editor, through the service's own route. Says so when it can't. */
export async function openInEditor(threadId: string, path: string, line?: number | null) {
  try {
    const editor = primaryEditor(await api.editors(), useUi.getState().preferredEditor);
    if (!editor) return;
    await api.openInEditor(threadId, { path, editor: editor.id, ...(line ? { line } : {}) });
  } catch (failure) {
    useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
  }
}
