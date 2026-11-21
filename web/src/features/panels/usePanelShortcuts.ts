import { useRef } from "react";
import { useCommand } from "../../product/shell/shortcuts";

/** Returns the plus button's ref. The keyboard routes to what the strip's buttons do: the plus button's list, and the maximise toggle (absent in the phone's sheet). */
export function usePanelShortcuts(
  openList: (at: { x: number; y: number }) => void,
  toggleMaximize: (() => void) | null,
) {
  const plus = useRef<HTMLButtonElement>(null);
  useCommand("rightPanel.new", () => {
    const rect = plus.current?.getBoundingClientRect();
    if (!rect) return false;
    openList({ x: rect.right - 200, y: rect.bottom + 4 });
  });
  useCommand("rightPanel.maximize", () => toggleMaximize?.(), () => toggleMaximize !== null);
  return plus;
}
