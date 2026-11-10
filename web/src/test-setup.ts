import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { forgetKept } from "./lib/keptData";
import { useThreadLogs } from "./product/state/threadLogs";
import { useConnection } from "./product/state/connection";
import { useUi } from "./product/state/ui";
import { useWorkspace } from "./product/state/workspace";

afterEach(() => {
  cleanup();
  forgetKept();
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useThreadLogs.setState(useThreadLogs.getInitialState(), true);
  useUi.setState(useUi.getInitialState(), true);
  useConnection.setState(useConnection.getInitialState(), true);
  localStorage.clear();
  sessionStorage.clear();
  window.location.hash = "";
  vi.unstubAllGlobals();
  stubBrowser();
});

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function stubBrowser() {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }));
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
}
stubBrowser();

Element.prototype.scrollTo ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function close(this: HTMLDialogElement) {
  this.open = false;
  this.dispatchEvent(new Event("close"));
};

