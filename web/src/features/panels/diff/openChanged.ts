import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { api } from "../api";
import { openInEditor } from "../files/openInEditor";

/**
 * The checkout-relative path of a file the diff names from the repository's root, or null when the
 * checkout doesn't hold it. `repoId` names one repository of a multi-repository thread, so the
 * service resolves the path in that repository's worktree; absent is its primary repository.
 */
export async function checkoutPath(threadId: string, cwd: string, path: string, signal?: AbortSignal, repoId?: string) {
  const resolved = await api.resolveThreadPath(threadId, { path, basis: "repository", cwd, ...(repoId === undefined ? {} : { repo_id: repoId }) }, signal);
  return resolved.checkout_path;
}

/** Opens the file in the person's editor, or in a file tab when the editor is not what is wanted (a binary file's Open). */
export async function openChanged(threadId: string, cwd: string, path: string, how: "editor" | "tab", repoId?: string) {
  try {
    const inside = await checkoutPath(threadId, cwd, path, undefined, repoId);
    // Another repository's file has no editor route of its own: it opens as that repository's tab,
    // which reads and writes with its `repo_id`. A path that repository's working tree does not hold
    // (a file deleted there, say) still opens the tab, whose read says why; the primary's stays a notice.
    if (inside === null && repoId === undefined) {
      useUi.getState().pushToast("Couldn't open it. That file is outside this thread's folder.");
      return;
    }
    const target = inside ?? path;
    if (how === "tab" || repoId !== undefined) useUi.getState().openFileTab({ kind: "thread", path: target, ...(repoId === undefined ? {} : { repoId }) });
    else await openInEditor(threadId, target);
  } catch (failure) {
    // A refused resolution (an untrusted repository, say) still opens the repository's tab, which
    // offers the trust prompt; an ordinary thread's file stays a notice.
    if (repoId !== undefined) useUi.getState().openFileTab({ kind: "thread", path, repoId });
    else useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
  }
}

