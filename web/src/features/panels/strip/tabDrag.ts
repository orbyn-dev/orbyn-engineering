import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import type { RightTab } from "../../../product/state/ui";
import { SLOT_PITCH } from "./tabs";

/** How far a press must move before it is a drag and not a click. */
const DRAG_START = 4;

/** The gap, 0 to `centers.length`, that a pointer at `x` is nearest: the slot the dragged tab would land before. */
export function dropGap(centers: readonly number[], x: number): number {
  let gap = 0;
  for (const center of centers) if (x > center) gap += 1;
  return gap;
}

/** Where the dragged tab lands among the shown tabs once it is lifted out and put into `gap`. */
export function landing(from: number, gap: number): number {
  return gap > from ? gap - 1 : gap;
}

/**
 * The slot's shift while another tab is dragged: the slots between where it was and where it will
 * land slide one pitch aside to make room, the rest stay.
 */
export function slotShift(at: number, from: number, to: number): number {
  if (to > from && at > from && at <= to) return -SLOT_PITCH;
  if (to < from && at >= to && at < from) return SLOT_PITCH;
  return 0;
}

/** The index to give `movePanelTab` so the dragged tab lands at `to` among the shown tabs; `order` is the whole row. */
export function moveIndex(order: readonly RightTab[], shown: readonly RightTab[], id: RightTab, to: number): number {
  const rest = order.filter((tab) => tab !== id);
  const others = shown.filter((tab) => tab !== id);
  const before = others[to];
  if (before !== undefined) return rest.indexOf(before);
  const last = others[others.length - 1];
  return last === undefined ? 0 : rest.indexOf(last) + 1;
}

export interface TabDrag {
  id: RightTab;
  /** Where the dragged tab was in the shown row, and where it will land. */
  from: number;
  to: number;
  dx: number;
  dy: number;
}

/**
 * Drag to reorder the strip with the pointer: the dragged tab lifts and follows the pointer, a line shows
 * where it will land and the other slots slide aside. Escape puts it back. Letting go commits the order.
 */
export function useTabDrag({
  list,
  shown,
  onMove,
}: {
  list: RefObject<HTMLElement | null>;
  shown: readonly RightTab[];
  onMove(id: RightTab, shownIndex: number): void;
}) {
  const [drag, setDrag] = useState<TabDrag | null>(null);
  const press = useRef<{ id: RightTab; pointerId: number; x: number; y: number; centers: number[]; from: number; active: boolean } | null>(null);
  // A drag that ends over its own slot is followed by a click, which must not open the tab.
  const justDragged = useRef(false);
  const latest = useRef({ shown, onMove });
  latest.current = { shown, onMove };

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const held = press.current;
      if (!held || event.pointerId !== held.pointerId) return;
      const dx = event.clientX - held.x;
      const dy = event.clientY - held.y;
      if (!held.active) {
        if (Math.hypot(dx, dy) < DRAG_START) return;
        held.active = true;
      }
      const to = landing(held.from, dropGap(held.centers, event.clientX));
      setDrag({ id: held.id, from: held.from, to, dx, dy });
    };
    const end = (event: PointerEvent, commit: boolean) => {
      const held = press.current;
      if (!held || event.pointerId !== held.pointerId) return;
      press.current = null;
      if (!held.active) return;
      justDragged.current = true;
      setTimeout(() => {
        justDragged.current = false;
      }, 0);
      const dx = event.clientX - held.x;
      const to = landing(held.from, dropGap(held.centers, event.clientX));
      setDrag(null);
      if (commit && to !== held.from && Math.abs(dx) >= DRAG_START) latest.current.onMove(held.id, to);
    };
    const up = (event: PointerEvent) => end(event, true);
    const cancel = (event: PointerEvent) => end(event, false);
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !press.current?.active) return;
      event.stopPropagation();
      press.current = null;
      justDragged.current = true;
      setTimeout(() => {
        justDragged.current = false;
      }, 0);
      setDrag(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", key, true);
    };
  }, []);

  const start = (id: RightTab) => (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0 || press.current) return;
    const slots = [...(list.current?.querySelectorAll<HTMLElement>("[data-tab]") ?? [])];
    const from = latest.current.shown.indexOf(id);
    if (from < 0) return;
    press.current = {
      id,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      centers: slots.map((slot) => {
        const rect = slot.getBoundingClientRect();
        return rect.left + rect.width / 2;
      }),
      from,
      active: false,
    };
  };

  return { drag, start, wasDrag: () => justDragged.current };
}
