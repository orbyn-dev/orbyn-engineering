import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { RESIZE_DELAY_MS } from "./browserModel";
import {
  RESIZE_HANDLES,
  RESIZE_KEY_HINT,
  draggedPage,
  nudgedPage,
  resizeKeyStep,
  type PageSize,
  type ResizeEdge,
} from "./viewportResize";

/** Where the device toolbar sends a page it is drawing at a size of its own. */
export interface PageResize {
  /** The size a drag or a run of key presses has reached, while it is under way. */
  live: PageSize | null;
  /** True while the device toolbar is open, which is what offers the handles. */
  handles: boolean;
  onPreview(size: PageSize | null): void;
  onCommit(size: PageSize): void;
}

/**
 * The handles on a page's box. A drag follows the pointer and commits once, on
 * release; the arrow keys step the page's own pixels and commit after a pause,
 * so a run of presses is one resize.
 */
export function ResizeHandles({
  page,
  stage,
  scale,
  onPreview,
  onCommit,
}: {
  page: PageSize;
  stage: PageSize;
  scale: number;
  onPreview(size: PageSize | null): void;
  onCommit(size: PageSize): void;
}) {
  const [dragging, setDragging] = useState<ResizeEdge | null>(null);
  /** The size a run of key presses reached, waiting to be committed as one. */
  const waiting = useRef<PageSize | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  /** Takes the window listeners of a drag that is under way back off. */
  const detach = useRef<(() => void) | null>(null);
  const stop = () => {
    clearTimeout(timer.current);
    timer.current = undefined;
  };
  const letGo = () => {
    detach.current?.();
    detach.current = null;
  };
  // A page is left where it is when the toolbar or the panel goes away.
  useEffect(
    () => () => {
      clearTimeout(timer.current);
      detach.current?.();
    },
    [],
  );
  const press = (edge: ResizeEdge) => (event: KeyboardEvent<HTMLButtonElement>) => {
    const step = resizeKeyStep(edge, event.key, event.shiftKey);
    if (!step) return;
    event.preventDefault();
    event.stopPropagation();
    const from = waiting.current ?? page;
    const next = nudgedPage(from, step);
    if (next.width === from.width && next.height === from.height) return;
    waiting.current = next;
    onPreview(next);
    stop();
    timer.current = setTimeout(() => {
      timer.current = undefined;
      const size = waiting.current;
      waiting.current = null;
      if (size) onCommit(size);
    }, RESIZE_DELAY_MS);
  };
  const grab = (edge: ResizeEdge) => (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    // The pointer wins over a size the keys had not sent yet.
    waiting.current = null;
    stop();
    letGo();
    const held = {
      edge,
      pointer: event.pointerId,
      start: page,
      x: event.clientX,
      y: event.clientY,
      latest: page,
      moved: false,
      stage: stage,
      scale: scale,
    };
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // The browser may refuse capture, and the drag still works without it.
    }
    // The pointer is watched on the window, since a drag leaves the edge it started on.
    const target = event.currentTarget;
    const pull = (move: PointerEvent) => {
      if (move.pointerId !== held.pointer) return;
      move.preventDefault();
      held.moved = true;
      held.latest = draggedPage(
        held.start,
        { x: move.clientX - held.x, y: move.clientY - held.y },
        held.stage,
        held.scale,
        held.edge,
      );
      onPreview(held.latest);
    };
    const release = () => {
      letGo();
      setDragging(null);
      try {
        target.releasePointerCapture(held.pointer);
      } catch {
        // Capture may already be gone.
      }
      const same = held.latest.width === held.start.width && held.latest.height === held.start.height;
      if (!held.moved || same) {
        onPreview(null);
        return;
      }
      onCommit(held.latest);
    };
    const cancel = () => {
      // A cancelled pointer never commits, so the page keeps the size it had.
      letGo();
      setDragging(null);
      onPreview(null);
    };
    const onUp = (up: PointerEvent) => {
      if (up.pointerId === held.pointer) release();
    };
    const onCancel = (gone: PointerEvent) => {
      if (gone.pointerId === held.pointer) cancel();
    };
    window.addEventListener("pointermove", pull);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    detach.current = () => {
      window.removeEventListener("pointermove", pull);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
    };
    setDragging(edge);
  };
  return (
    <>
      {RESIZE_HANDLES.map(({ edge, name }) => (
        <button
          key={edge}
          type="button"
          className="browser-handle"
          data-edge={edge}
          data-dragging={dragging === edge || undefined}
          aria-label={`${name}. ${RESIZE_KEY_HINT}`}
          title={`${name}. ${RESIZE_KEY_HINT}`}
          onPointerDown={grab(edge)}
          onKeyDown={press(edge)}
        />
      ))}
    </>
  );
}
