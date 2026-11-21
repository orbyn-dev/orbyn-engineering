/** The state one drag of the panel's left edge carries, and the page mark that keeps the drag from selecting text. */
export interface PanelDrag {
  panel: HTMLElement;
  handle: HTMLElement;
  pointerId: number;
  right: number;
  layoutKey: string;
  startWidth: string;
  /** The width this drag last wrote to the panel, or null before the first move. */
  writtenWidth: string | null;
  current: number;
}

/** Marks the page while an edge is dragged, so no drag can start a selection over its text. */
export const setPageResizing = (resizing: boolean) => {
  if (resizing) document.documentElement.dataset.resizing = "panel";
  else delete document.documentElement.dataset.resizing;
};
