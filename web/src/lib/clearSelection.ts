import { useEffect } from "react";

/** Drops the document's text selection, if there is one. */
export function clearSelection(): void {
  const selection = window.getSelection();
  if (selection && selection.rangeCount > 0 && !selection.isCollapsed) selection.removeAllRanges();
}

/**
 * A text selection outlives the page it was made on: the DOM that held it is replaced, but the selection stays on the
 * window, and WebKit keeps drawing its system affordance (the round Writing Tools icon on macOS) over the next page.
 * The selection is dropped when `key` changes and when the component unmounts.
 */
export function useClearSelectionOnLeave(key: string): void {
  useEffect(() => clearSelection, [key]);
}
