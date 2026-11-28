import { toFailure } from "../../../lib/useAsync";
import { useUi, type FileTarget } from "../../../product/state/ui";
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

/**
 * Opens the file in the person's editor, or in a file tab when the editor is not what is wanted (a binary file's Open).
 * `repoId` names another repository of a multi-repository thread, whose tab reads and writes with it; `primaryRepoId`
 * names the thread's primary repository, whose tab names it and still reads and writes as the thread's own.
 */
export async function openChanged(threadId: string, cwd: string, path: string, how: "editor" | "tab", repoId?: string, primaryRepoId?: string) {
  try {
    const inside = await checkoutPath(threadId, cwd, path, undefined, repoId);
    // Another repository's file has no editor route of its own: it opens as that repository's tab,
    // which reads and writes with its `repo_id`. A path that repository's working tree does not hold
    // (a file deleted there, say) still opens the tab, whose read says why; the primary's and an
    // ordinary thread's stay a notice, as they always have.
    if (inside === null && repoId === undefined) {
      useUi.getState().pushToast("Couldn't open it. That file is outside this thread's folder.");
      return;
    }
    const target = inside ?? path;
    // The primary repository's file is still the thread's own tab: it names the repository, and its reads and writes leave
    // the repository out (absent `repo_id` is the primary).
    if (how === "tab" || repoId !== undefined || primaryRepoId !== undefined) {
      const tab: FileTarget =
        primaryRepoId !== undefined ? { kind: "thread", path: target, repoId: primaryRepoId, primary: true }
          : repoId !== undefined ? { kind: "thread", path: target, repoId }
            : { kind: "thread", path: target };
      useUi.getState().openFileTab(tab);
    } else await openInEditor(threadId, target);
  } catch (failure) {
    // A refused resolution (an untrusted repository, say) still opens the repository's tab, which
    // offers the trust prompt; an ordinary thread's file stays a notice.
    if (repoId !== undefined) useUi.getState().openFileTab({ kind: "thread", path, repoId });
    else useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
  }
}

