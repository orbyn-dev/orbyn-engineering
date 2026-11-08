import { useEffect, useId, useLayoutEffect, useRef, type RefObject } from "react";
import { captureFocusReturn, moveFocus, tabbables, watchOutside, type CloseCause, type FocusReturn, type PopoverKind } from "./focusReturn";

export interface PopoverPane {
  /** The pane's element. */
  root: RefObject<HTMLElement | null>;
  /** Puts focus on the pane's one stop (its selected rail button, its search field). */
  focus(): void;
}

export interface PopoverOptions {
  /** How Tab behaves: see `PopoverKind`. */
  kind: PopoverKind;
  /** The popover's own element. */
  root: RefObject<HTMLElement | null>;
  /** The button that opens it. Without one, the element that had focus when it opened. */
  trigger?(): HTMLElement | null | undefined;
  /** False while it is closed, for a popover that stays mounted. */
  active?: boolean;
  /** Set by `Presence` while it fades out: it listens to nothing. */
  closing?: boolean;
  /**
   * A popover of several panes (a rail and a list, say), in Tab order. Each pane is one stop: Tab
   * goes to the next pane's `focus()`, Shift+Tab to the previous one's. Tab past the last pane and
   * Shift+Tab before the first close it by the usual rule. Focus in no pane follows `kind`.
   */
  panes?: readonly PopoverPane[];
  /** Hides it. Focus has already moved; this runs once per opening. */
  onClose(): void;
}

/**
 * The shared close and focus rule for a popover that is not a `Menu`: Escape, Tab, Shift+Tab, a
 * real user action outside and a route change. See `focusReturn.ts` for where focus goes.
 * Returns `dismiss`, for closing from inside (choosing something) with the same focus rule.
 */
export function usePopover({ kind, root, trigger, active = true, closing, panes, onClose }: PopoverOptions) {
  const state = useRef<{ focusReturn: FocusReturn | null; moved: boolean }>({ focusReturn: null, moved: false });
  const id = useId();
  const markedRoot = useRef<HTMLElement | null>(null);
  // Taken as it opens, while rendering, before anything inside takes focus.
  if (!active) state.current = { focusReturn: null, moved: false };
  else if (!state.current.focusReturn) state.current.focusReturn = captureFocusReturn(trigger?.() ?? undefined);

  const dismiss = (cause: CloseCause, target?: EventTarget | null) => {
    const current = state.current;
    if (!active || closing || current.moved || !current.focusReturn) return;
    current.moved = true;
    moveFocus(current.focusReturn, cause, target);
    onClose();
  };
  const latest = useRef({ dismiss, panes, trigger, kind });
  latest.current = { dismiss, panes, trigger, kind };

  // Keep markers across ordinary renders: a nested portal reads its ancestor's marker during layout.
  useLayoutEffect(() => {
    const element = active ? root.current : null;
    const previous = markedRoot.current;
    if (previous !== element) {
      if (previous?.getAttribute("data-popover-root") === id) {
        previous.removeAttribute("data-popover-root");
        previous.removeAttribute("data-popover-owner");
      }
      markedRoot.current = element;
    }
    if (!element) return;
    element.setAttribute("data-popover-root", id);
    // A popover opened from inside another names it, so only that one counts as inside it.
    const owner = latest.current.trigger?.()?.closest<HTMLElement>("[data-popover-root]")?.dataset.popoverRoot;
    if (owner) element.setAttribute("data-popover-owner", owner);
    else element.removeAttribute("data-popover-owner");
  });

  // Cleanup belongs to this hook's lifetime, not each render, and only releases markers it still owns.
  useLayoutEffect(() => () => {
    const element = markedRoot.current;
    if (element?.getAttribute("data-popover-root") === id) {
      element.removeAttribute("data-popover-root");
      element.removeAttribute("data-popover-owner");
    }
    markedRoot.current = null;
  }, [id]);

  useEffect(() => {
    if (!active || closing) return;
    const inRoot = (target: EventTarget | null) => target instanceof Node && !!root.current?.contains(target);
    const inTrigger = (target: EventTarget | null) => target instanceof Node && !!latest.current.trigger?.()?.contains(target);
    const onKeyDown = (event: KeyboardEvent) => {
      const { dismiss: close, panes: stops, kind: mode } = latest.current;
      const target = event.target;
      const element = root.current;
      // Focus nowhere (its field was disabled under it) still lets Escape close it.
      const nowhere = target === document.body || target === document.documentElement || target === document;
      if (!element || event.defaultPrevented || !(inRoot(target) || inTrigger(target) || (nowhere && event.key === "Escape"))) return;
      const stopEvent = () => {
        event.preventDefault();
        event.stopPropagation();
      };
      if (event.key === "Escape") {
        stopEvent();
        close("escape");
        return;
      }
      if (event.key !== "Tab") return;
      const pane = stops?.findIndex((entry) => target instanceof Node && !!entry.root.current?.contains(target)) ?? -1;
      if (stops && pane >= 0) {
        const next = stops[pane + (event.shiftKey ? -1 : 1)];
        stopEvent();
        if (next) next.focus();
        else close(event.shiftKey ? "shiftTab" : "tab");
        return;
      }
      if (mode !== "fields") {
        stopEvent();
        close(event.shiftKey ? "shiftTab" : "tab");
        return;
      }
      const fields = tabbables(element);
      const onTrigger = inTrigger(target) && !inRoot(target);
      const at = target instanceof HTMLElement ? fields.indexOf(target) : -1;
      if (event.shiftKey) {
        if (onTrigger) {
          stopEvent();
          close("shiftTab");
        } else if (at <= 0) {
          // On the first field, or on the popover itself: back to the trigger, still open.
          stopEvent();
          if (!state.current.focusReturn?.toTrigger()) close("shiftTab");
        }
      } else if (onTrigger || at === -1) {
        // From the trigger into the popover, wherever it sits in the page.
        stopEvent();
        if (fields[0]) fields[0].focus();
        else close("tab");
      } else if (at === fields.length - 1) {
        stopEvent();
        close("tab");
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    // A menu, or a popover opened from inside this one, is a portal: pressing in it is not leaving.
    const inMenu = (target: EventTarget | null) => {
      if (!(target instanceof Element)) return false;
      if (target.closest("[data-menu-root]")) return true;
      let popover = target.closest<HTMLElement>("[data-popover-root]");
      for (let depth = 0; popover && depth < 8; depth += 1) {
        if (popover.dataset.popoverRoot === id) return true;
        const owner = popover.dataset.popoverOwner;
        popover = owner ? document.querySelector<HTMLElement>(`[data-popover-root="${CSS.escape(owner)}"]`) : null;
      }
      return false;
    };
    const stop = watchOutside(
      (target) => inRoot(target) || inTrigger(target) || inMenu(target),
      (cause, target) => latest.current.dismiss(cause, target),
    );
    return () => {
      stop();
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [active, closing, root, id]);

  // Closed from outside (the trigger toggling it): the same as Escape, unless focus moved on already.
  useLayoutEffect(() => {
    const current = state.current;
    if (!closing || current.moved || !current.focusReturn) return;
    current.moved = true;
    const focused = document.activeElement;
    if (!focused || focused === document.body || focused.closest("[data-popover-root], [data-closing]")) current.focusReturn.restore();
  }, [closing]);

  return { dismiss };
}
