import { DRAG_DISTANCE } from "./pressGesture";

/** How long a touch stays down before it opens the row's context menu. */
export const LONG_PRESS_MS = 500;

export interface PressPoint {
  pointerType: string;
  pointerId: number;
  clientX: number;
  clientY: number;
}

/**
 * A touch held still opens the context menu, which has no hover to reveal the row's actions. Moving as far as a drag
 * takes (the same distance trackPress uses to start one) cancels it, so a drag or a scroll never opens a menu. A
 * browser that sends its own contextmenu for a long press (Android) opens the same menu once, not twice.
 */
export function createLongPress(onFire: (at: { clientX: number; clientY: number }) => void, ms = LONG_PRESS_MS) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let reset: ReturnType<typeof setTimeout> | undefined;
  let start = { x: 0, y: 0, id: -1 };
  let fired = false;
  let touching = false;
  const stop = () => {
    clearTimeout(timer);
    timer = undefined;
  };
  return {
    down(point: PressPoint) {
      clearTimeout(reset);
      fired = false;
      touching = point.pointerType === "touch";
      stop();
      if (!touching) return;
      start = { x: point.clientX, y: point.clientY, id: point.pointerId };
      timer = setTimeout(() => {
        timer = undefined;
        fired = true;
        onFire({ clientX: start.x, clientY: start.y });
      }, ms);
    },
    move(point: PressPoint) {
      if (timer && point.pointerId === start.id && Math.hypot(point.clientX - start.x, point.clientY - start.y) >= DRAG_DISTANCE) stop();
    },
    /** The pointer went up or was cancelled. The click that follows a long press still sees `fired`; it is cleared after. */
    end() {
      stop();
      clearTimeout(reset);
      reset = setTimeout(() => {
        fired = false;
        touching = false;
      }, 0);
    },
    /** True once for the click that ends a long press: that is not a click on the row. */
    consumeClick(): boolean {
      const was = fired;
      fired = false;
      return was;
    },
    /** What a contextmenu event should do: "open" the menu, or "skip" because this press already did. */
    contextMenu(): "open" | "skip" {
      if (!touching && !fired) return "open";
      stop();
      if (fired) return "skip";
      fired = true;
      return "open";
    },
    dispose() {
      stop();
      clearTimeout(reset);
    },
  };
}
