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
export function useRouteFocus(
  routeKey: string,
  routeName: string,
  ready: boolean,
  main: React.RefObject<HTMLElement | null>,
  coveredKey: string,
) {
  const first = useRef(true);
  const previous = useRef({ key: routeKey, name: routeName });
  // What had focus, and which page was showing, when settings opened.
  const opener = useRef<{ element: Element | null; key: string } | null>(null);
  // The route of the last commit, ready or not: `previous` follows settled pages only, so a Settings
  // session that never became ready leaves no trace there.
  const last = useRef(routeName);
  useEffect(() => {
    const before = previous.current;
    // A route that differs from the last commit is a departure, even when it is also the last page focus
    // completed on: leaving a page while its screen was still arriving never reached `previous`, so
    // coming back to it looked like the page that never changed.
    const departed = last.current !== routeName;
    const entering = routeName === "settings" && departed;
    last.current = routeName;
    // What opened Settings is recorded the moment the route enters it, never when its screen arrives:
    // the wait for a first chunk is exactly when tabbing moves the keyboard, and closing Settings must
    // return to the control that opened it rather than to whatever the tab reached meanwhile. A new
    // session records its own opener, replacing one a session that never came back left waiting.
    if (entering) {
      const active = document.activeElement;
      // The page it covers is the one drawn behind it, which the shell already knows, not the last page
      // focus completed on: a screen whose chunk is still arriving has replaced that page already, and
      // closing Settings restores the opener only while it names the page that is showing.
      opener.current = { element: active && active !== document.body ? active : null, key: coveredKey };
    }
    // The opener belongs to the page Settings covers, so it is read whenever that page is the route again:
    // the session that recorded it is ending, or ended before either screen was ready and is still owed a
    // hand-back when the page behind arrives.
    const back = routeName === "settings" ? null : opener.current;
    if (!ready) {
      // Nothing on the page can take the focus yet, so nothing is handed back. The opener is kept while
      // the page it covered is the page still arriving, because that page is where it has to be handed
      // back; it is dropped once the route is some other page, since nothing can bring focus back there.
      if (back !== null && back.key !== routeKey) opener.current = null;
      return;
    }
    // The page behind is the page showing again, so its opener is settled on this commit either way.
    if (back !== null) opener.current = null;
    const root = main.current;
    if (back !== null && back.element instanceof HTMLElement && back.key === routeKey && back.element.isConnected && !back.element.closest("[hidden]")) {
      // Closing settings returns to what opened it, if it is still there and the page behind is showing.
      // That holds whether or not Settings' screen arrived, and whether or not the page behind had: Escape
      // while a chunk is still coming closes it from the page that never left, so the wait must not cost
      // the person the control they opened it from.
      previous.current = { key: routeKey, name: routeName };
      // The first handoff is spent on this restore, even though it lands on a control rather than the
      // screen: the destination after it takes its own title, like every later page, instead of being
      // skipped as the page the app opened on.
      first.current = false;
      back.element.focus({ preventScroll: true });
      return;
    }
    if (!root) return;
    // The screen only became ready: the phone's sheet closed over the same page, so nothing moved and
    // focus stays where it was given (the title bar button, or the sheet's own Close). A page that
    // changed is followed below, and a page the route left and came back to counts as changed even
    // though it is the last page focus completed on too: its screen mounted again.
    if (!first.current && !departed && before.key === routeKey && before.name === routeName) return;
    previous.current = { key: routeKey, name: routeName };
    // A page kept mounted behind settings is hidden, so it never takes the focus.
    const visible = (selector: string) => [...root.querySelectorAll<HTMLElement>(selector)].find((element) => !element.closest("[hidden]")) ?? null;
    const target = visible("[data-autofocus]") ?? (first.current ? null : visible("[data-page-title]"));
    first.current = false;
    (target ?? root).focus({ preventScroll: true });
    root.scrollTop = 0;
  }, [routeKey, routeName, ready, main, coveredKey]);
}
