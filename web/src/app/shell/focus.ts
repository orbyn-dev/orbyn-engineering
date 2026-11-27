import { useEffect, useRef } from "react";

/**
 * Keeps the toast stack clear of the composer: `--toast-lift` is how far its top edge stands
 * above the window's bottom, so the stack sits just above it.
 */
export function useToastClearance(key: string) {
  useEffect(() => {
    const root = document.documentElement;
    let composer: Element | null = null;
    let active = true;
    const apply = () => {
      const rect = composer?.getBoundingClientRect();
      const lift = rect && rect.height > 0 ? Math.max(0, window.innerHeight - rect.top + 8) : 0;
      root.style.setProperty("--toast-lift", `${lift}px`);
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => {
      if (!active) return;
      if (composer && !composer.isConnected) find();
      else apply();
    });
    const find = () => {
      if (composer) observer?.unobserve(composer);
      composer = document.querySelector(".composer");
      if (composer) observer?.observe(composer);
      apply();
    };
    // Detect replacement even when a detached composer can no longer resize. While
    // it is connected, streaming DOM changes need only a connectivity check.
    const mutations = new MutationObserver((records) => {
      if (composer?.isConnected) return;
      if (composer || records.some((record) => Array.from(record.addedNodes).some(
        (node) => node instanceof Element && (node.matches(".composer") || node.querySelector(".composer")),
      ))) find();
    });
    mutations.observe(document.body, { childList: true, subtree: true });
    find();
    // The composer glides to the bottom when a first message is sent; its size need not change.
    const update = () => {
      if (composer && !composer.isConnected) find();
      else apply();
    };
    window.addEventListener("resize", update);
    window.addEventListener("transitionend", update, true);
    return () => {
      active = false;
      mutations.disconnect();
      observer?.disconnect();
      window.removeEventListener("resize", update);
      window.removeEventListener("transitionend", update, true);
      root.style.removeProperty("--toast-lift");
    };
  }, [key]);
}

/** Moves focus into the new screen, so keyboard and screen reader users follow it. */
export function useRouteFocus(routeKey: string, routeName: string, ready: boolean, main: React.RefObject<HTMLElement | null>) {
  const first = useRef(true);
  const previous = useRef({ key: routeKey, name: routeName });
  // What had focus, and which page was showing, when settings opened.
  const opener = useRef<{ element: Element | null; key: string } | null>(null);
  useEffect(() => {
    const before = previous.current;
    // What opened Settings is recorded the moment the route enters it, never when its screen arrives:
    // the wait for a first chunk is exactly when tabbing moves the keyboard, and closing Settings must
    // return to the control that opened it rather than to whatever the tab reached meanwhile.
    if (routeName === "settings" && before.name !== "settings" && opener.current === null) {
      const active = document.activeElement;
      opener.current = { element: active && active !== document.body ? active : null, key: before.key };
    }
    if (!ready) return;
    const root = main.current;
    if (!root) return;
    // The screen only became ready: the phone's sheet closed over the same page, so nothing moved and
    // focus stays where it was given (the title bar button, or the sheet's own Close). A page that
    // changed is followed below.
    if (!first.current && before.key === routeKey && before.name === routeName) return;
    previous.current = { key: routeKey, name: routeName };
    if (routeName !== "settings" && before.name === "settings") {
      // Closing settings returns to what opened it, if it is still there and the page behind is showing.
      const back = opener.current;
      opener.current = null;
      if (back?.element instanceof HTMLElement && back.element.isConnected && back.key === routeKey && !back.element.closest("[hidden]")) {
        back.element.focus({ preventScroll: true });
        return;
      }
    }
    // A page kept mounted behind settings is hidden, so it never takes the focus.
    const visible = (selector: string) => [...root.querySelectorAll<HTMLElement>(selector)].find((element) => !element.closest("[hidden]")) ?? null;
    const target = visible("[data-autofocus]") ?? (first.current ? null : visible("[data-page-title]"));
    first.current = false;
    (target ?? root).focus({ preventScroll: true });
    root.scrollTop = 0;
  }, [routeKey, routeName, ready, main]);
}
