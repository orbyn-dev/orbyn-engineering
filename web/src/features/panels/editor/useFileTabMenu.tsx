import { Copy, Pencil, X } from "lucide-react";
import { api } from "../../../api/client";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { launchableEditors, primaryEditor } from "../../../product/composer/editorPreferences";
import { navigate } from "../../../product/state/route";
import { parseFileTab, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import type { MenuItem } from "../../../ui/Menu";
import { useExplorer } from "../../workspace";

/**
 * The ⋯ menu of an open file (editor-tab-menu): the path in two forms, the file in an outside editor (the picker of
 * installed ones, the favourite marked) and Close tab. Reveal in the left bar is not here: the left bar has no way to
 * be told to show a file yet.
 */
export function useFileTabMenu(threadId: string, tab: string | null, close: () => void): MenuItem[] | null {
  const target = tab ? parseFileTab(tab) : null;
  const editors = useAsync("editors", (signal) => api.editors(signal));
  const preferred = useUi((state) => state.preferredEditor);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd ?? "");
  const worktree = useExplorer((state) => (target?.kind === "explorer" ? state.worktrees[target.projectId]?.data?.worktrees.find((one) => one.id === target.worktreeId) : undefined));
  const pushToast = useUi((state) => state.pushToast);
  const closeItem: MenuItem = { label: "Close tab", icon: <X size={14} aria-hidden />, onSelect: close };
  if (!target) return null;
  if (target.kind === "worktree") return [closeItem];
  const relative = target.path;
  const root = target.kind === "thread" ? cwd : (worktree?.path ?? "");
  const absolute = root ? `${root.replace(/\/$/, "")}/${relative}` : null;
  const copy = (text: string, done: string) =>
    navigator.clipboard?.writeText(text).then(
      () => pushToast(done),
      () => pushToast("Couldn't copy the path"),
    );
  // The thread's own file is named relative to its folder; an explorer file to the worktree, and only the thread's can be handed to an editor.
  const own = target.kind === "thread" ? target.path : null;
  const list = editors.data ?? [];
  const primary = primaryEditor(list, preferred);
  const open = async (id: string, remember: boolean) => {
    if (own === null) return;
    try {
      await api.openInEditor(threadId, { path: own, editor: id });
      // Choosing another editor here opens the file in it once; the favourite stays.
      if (remember) useUi.getState().setPrefs({ preferredEditor: id });
    } catch (failure) {
      pushToast(`Couldn't open it. ${toFailure(failure).message}`);
    }
  };
  const picker: MenuItem[] = [
    ...launchableEditors(list).map((editor) => ({
      label: editor.name,
      selected: editor.id === primary?.id,
      ...(editor.id === primary?.id ? { shortcut: "Favourite" } : {}),
      onSelect: () => void open(editor.id, false),
    })),
    { label: "Choose favourite in Settings…", separatorBefore: true, onSelect: () => navigate({ name: "settings", section: "general" }) },
  ];
  return [
    { label: "Copy path", icon: <Copy size={14} aria-hidden />, disabled: absolute === null, onSelect: () => absolute && void copy(absolute, "Path copied") },
    { label: "Copy relative path", icon: <Copy size={14} aria-hidden />, onSelect: () => void copy(relative, "Path copied") },
    ...(own !== null && primary
      ? [{ label: "Open in an outside editor", icon: <Pencil size={14} aria-hidden />, separatorBefore: true, children: picker }]
      : []),
    { ...closeItem, separatorBefore: true },
  ];
}
