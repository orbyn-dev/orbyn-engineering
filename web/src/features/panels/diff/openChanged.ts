import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { api } from "../api";
import { openInEditor } from "../files/openInEditor";

/** The checkout-relative path of a file the diff names from the repository's root, or null when the thread's folder doesn't hold it. */
export async function checkoutPath(threadId: string, cwd: string, path: string, signal?: AbortSignal) {
  const resolved = await api.resolveThreadPath(threadId, { path, basis: "repository", cwd }, signal);
  return resolved.checkout_path;
}

/** Opens the file in the person's editor, or in a file tab when the editor is not what is wanted (a binary file's Open). */
export async function openChanged(threadId: string, cwd: string, path: string, how: "editor" | "tab") {
  try {
    const inside = await checkoutPath(threadId, cwd, path);
    if (inside === null) {
      useUi.getState().pushToast("Couldn't open it. That file is outside this thread's folder.");
      return;
    }
    if (how === "tab") useUi.getState().openFileTab({ kind: "thread", path: inside });
    else await openInEditor(threadId, inside);
  } catch (failure) {
    useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
  }
}

