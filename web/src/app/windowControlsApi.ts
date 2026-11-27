import { bridge } from "../api/bridge";

/** The Maximize button's box in device pixels from the client's top-left (CSS pixels times devicePixelRatio, so the window's zoom is in it), for the shell's native hit test. */
export type MaximizeRect = { x: number; y: number; width: number; height: number };

/**
 * Calls a shell command through the page's bridge. The window controls are chrome, not a feature:
 * a command that fails, or a page without the bridge, quietly does nothing.
 */
function quietly(command: string, args?: Record<string, unknown>): void {
  const internals = bridge();
  const invoke = internals?.invoke;
  if (typeof invoke !== "function") return;
  try {
    const answer = args === undefined ? invoke.call(internals, command) : invoke.call(internals, command, args);
    void Promise.resolve(answer).catch(() => undefined);
  } catch {
    // The shell refused the command synchronously; there is nothing for the page to do.
  }
}

export const minimizeWindow = (): void => quietly("plugin:window|minimize");
export const maximizeWindow = (): void => quietly("plugin:window|toggle_maximize");
export const closeWindow = (): void => quietly("plugin:window|close");

/** True while the shell's window is maximized. No bridge or a failed answer reads as not maximized. */
export async function isWindowMaximized(): Promise<boolean> {
  const internals = bridge();
  const invoke = internals?.invoke;
  if (typeof invoke !== "function") return false;
  try {
    return (await invoke.call(internals, "plugin:window|is_maximized")) === true;
  } catch {
    return false;
  }
}

/** Tells the shell where Maximize sits, so Windows can show its snap flyout there; null clears it. */
export const setMaximizeRect = (rect: MaximizeRect | null): void =>
  quietly("window_set_maximize_rect", { rect });

/** Runs an item from the integrated window menu in the shell. */
export const runWindowMenuItem = (id: string): void => quietly("window_menu_run", { id });
