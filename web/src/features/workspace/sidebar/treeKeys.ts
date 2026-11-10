import type { RefObject } from "react";
import type { Thread } from "../../../api/types";
import type { RowActions } from "./ThreadRow";

/** What the list's key handler reads from the sidebar. */
export interface TreeKeyContext {
  tree: RefObject<HTMLDivElement | null>;
  threads: Record<string, Thread>;
  setRenaming(id: string): void;
  actions: RowActions;
}

/** The keys the thread list answers: arrows move focus, F2 renames, the menu key opens the row menu. */
export function handleTreeKey(
  event: React.KeyboardEvent,
  { tree, threads, setRenaming, actions }: TreeKeyContext,
) {
  const target = event.target as HTMLElement;
  if (target.tagName === "INPUT") return;
  const rows = [...(tree.current?.querySelectorAll<HTMLElement>("a.thread-row, button.group-head") ?? [])];
  const at = rows.indexOf(target);
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    if (at < 0) return;
    event.preventDefault();
    rows[Math.min(rows.length - 1, Math.max(0, at + (event.key === "ArrowDown" ? 1 : -1)))]?.focus();
    return;
  }
  const id = target.closest<HTMLElement>("[data-thread-item]")?.dataset.threadItem;
  if ((event.key === "ArrowRight" || event.key === "ArrowLeft") && target.matches("button.group-head")) {
    const open = target.getAttribute("aria-expanded") === "true";
    if (open === (event.key === "ArrowLeft")) target.click();
    event.preventDefault();
    return;
  }
  const thread = id ? threads[id] : undefined;
  if (!thread) return;
  if (event.key === "F2") {
    event.preventDefault();
    setRenaming(thread.id);
  } else if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
    event.preventDefault();
    const rect = target.getBoundingClientRect();
    actions.onMenu({ clientX: rect.left + 12, clientY: rect.bottom }, thread);
  }
}
