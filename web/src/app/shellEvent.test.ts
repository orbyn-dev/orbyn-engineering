import { afterEach, describe, expect, it } from "vitest";
import { onDataChanged } from "../api/shell";
import { useUi } from "../product/state/ui";
import { onShellEvent } from "./shell";

describe("onShellEvent", () => {
  afterEach(() => {
    window.location.hash = "";
    useUi.getState().showInBrowser(null);
  });

  it("shows the preview the service opened for a link from another app", () => {
    onShellEvent({ type: "show_preview", thread_id: "t1", tab_id: "tab-1" });
    expect(window.location.hash).toBe("#/threads/t1");
    const state = useUi.getState();
    expect(state.previewRequest).toEqual({ tabId: "tab-1" });
    expect(state.panelOpen).toBe(true);
    expect(state.panelTab).toBe("browser");
  });

  it("tells listeners what data the service refreshed", () => {
    const heard: string[] = [];
    const stop = onDataChanged((what) => heard.push(what));
    onShellEvent({ type: "data_changed", what: "pulls" });
    stop();
    onShellEvent({ type: "data_changed", what: "models:claude" });
    expect(heard).toEqual(["pulls"]);
  });
});
