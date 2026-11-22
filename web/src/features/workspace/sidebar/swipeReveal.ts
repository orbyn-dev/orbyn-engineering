import type { PressPoint } from "./longPress";

/** How far a touch has to travel sideways before it is a swipe. */
export const SWIPE_DISTANCE = 40;

/**
 * Touch has no hover, so a thread row shows its Snooze and Settle strip when the row is swiped left, and hides it again on
 * a swipe right. Only a touch counts, and only a move that is mostly sideways, so a scroll never reveals anything. The click
 * that ends a swipe is not a click on the row (`consumeClick`).
 */
export function createSwipeReveal(onChange: (open: boolean) => void) {
  let start: { x: number; y: number; id: number } | null = null;
  let open = false;
  let swiped = false;
  let reset: ReturnType<typeof setTimeout> | undefined;
  const set = (next: boolean) => {
    if (open === next) return;
    open = next;
    onChange(next);
  };
  return {
    down(point: PressPoint) {
      clearTimeout(reset);
      swiped = false;
      start = point.pointerType === "touch" ? { x: point.clientX, y: point.clientY, id: point.pointerId } : null;
    },
    move(point: PressPoint) {
      if (!start || swiped || point.pointerId !== start.id) return;
      const dx = point.clientX - start.x;
      const dy = point.clientY - start.y;
      if (Math.abs(dx) < SWIPE_DISTANCE || Math.abs(dx) < Math.abs(dy) * 1.5) return;
      swiped = true;
      set(dx < 0);
    },
    end() {
      start = null;
      clearTimeout(reset);
      reset = setTimeout(() => {
        swiped = false;
      }, 0);
    },
    /** True once for the click that ends a swipe. */
    consumeClick(): boolean {
      const was = swiped;
      swiped = false;
      return was;
    },
    /** A tap elsewhere, or a press on one of the strip's buttons, puts the strip away. */
    close() {
      set(false);
    },
    isOpen: () => open,
    dispose() {
      clearTimeout(reset);
    },
  };
}
