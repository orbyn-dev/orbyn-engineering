/** Two clicks make a double click when they are this close in time and place (the system default is about this). */
export const DOUBLE_CLICK_MS = 500;
export const DOUBLE_CLICK_PX = 4;

type Invoke = (command: string) => unknown;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: { invoke: (command: string) => unknown };
  }
}

/** Tracks the last click on the bar's empty space; a click anywhere else forgets it. */
export function doubleClickTracker(now: () => number = () => performance.now()) {
  let last: { at: number; x: number; y: number } | null = null;
  return {
    /** True when this click completes a double click. */
    click(x: number, y: number): boolean {
      const at = now();
      const hit = last !== null && at - last.at <= DOUBLE_CLICK_MS &&
        Math.hypot(x - last.x, y - last.y) <= DOUBLE_CLICK_PX;
      last = hit ? null : { at, x, y };
      return hit;
    },
    reset() { last = null; },
  };
}

/**
 * The title bar's empty space drags the window, and a real double click on it zooms the window. This is ours
 * rather than Tauri's `data-tauri-drag-region` script, which takes the browser's click count: that count can
 * reach 2 across a button and the empty space, so two quick clicks at different spots zoomed the window.
 * Buttons and menus never reach the layer, and a click anywhere else forgets the first click.
 * Returns a stop function.
 */
export function attachBarDrag(
  layer: HTMLElement,
  invoke: Invoke | undefined = window.__TAURI_INTERNALS__?.invoke.bind(window.__TAURI_INTERNALS__),
  now?: () => number,
): () => void {
  const tracker = doubleClickTracker(now);
  const onDown = (event: MouseEvent) => {
    if (event.target !== layer) { tracker.reset(); return; }
    if (event.button !== 0 || !invoke) return;
    event.preventDefault();
    if (tracker.click(event.clientX, event.clientY)) void invoke("plugin:window|internal_toggle_maximize");
    else void invoke("plugin:window|start_dragging");
  };
  document.addEventListener("mousedown", onDown, true);
  return () => document.removeEventListener("mousedown", onDown, true);
}
