import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { forgetKept } from "./lib/keptData";
import { resetLimitsStore } from "./lib/limitsStore";
import { useThreadLogs } from "./product/state/threadLogs";
import { useConnection } from "./product/state/connection";
import { useUi } from "./product/state/ui";
import { useWorkspace } from "./product/state/workspace";

afterEach(() => {
  cleanup();
  forgetKept();
  resetLimitsStore();
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useThreadLogs.setState(useThreadLogs.getInitialState(), true);
  useUi.setState(useUi.getInitialState(), true);
  useConnection.setState(useConnection.getInitialState(), true);
  localStorage.clear();
  sessionStorage.clear();
  window.location.hash = "";
  vi.unstubAllGlobals();
  installBrowserStandIns();
});

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

/** jsdom makes no media queries: a window this wide matches none of them. */
const mediaQueryList = (query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener: () => undefined,
  removeEventListener: () => undefined,
  addListener: () => undefined,
  removeListener: () => undefined,
  dispatchEvent: () => false,
});

/**
 * jsdom has no `matchMedia` and no `ResizeObserver`, so these stand in for a browser's.
 *
 * They are plain properties of the window, not `vi.stubGlobal` stubs. The stub registry remembers a
 * global that jsdom does not have as `undefined`, so `vi.unstubAllGlobals()` put `undefined` back
 * and `window.matchMedia` stopped being a function until this setup installed the stand-in again.
 * The files that call `vi.unstubAllGlobals()` in their own `afterEach` therefore take it away for
 * the rest of that teardown, including the `cleanup()` above, which flushes the React effects a
 * test left pending. That is how RepositoriesSection.test failed under full-suite load with
 * "window.matchMedia is not a function" (issue #1065). A plain property survives every unstub, and
 * a test that stubs one of them still gets this stand-in back from `vi.unstubAllGlobals()`.
 */
function installBrowserStandIns() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => mediaQueryList(query),
  });
  Object.defineProperty(window, "ResizeObserver", {
    configurable: true,
    writable: true,
    value: ResizeObserverStub,
  });
}
installBrowserStandIns();

Element.prototype.scrollTo ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function close(this: HTMLDialogElement) {
  this.open = false;
  this.dispatchEvent(new Event("close"));
};
