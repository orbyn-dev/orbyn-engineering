import { beforeEach, describe, expect, it, vi } from "vitest";
import { DRAFT_LAYOUT, useUi } from "./ui";

const ui = () => useUi.getState();

beforeEach(() => {
  ui().setLayoutKey(DRAFT_LAYOUT);
  useUi.setState({ layouts: {}, panelOpen: null, panelTab: null, terminalOpen: false, panelWidth: null, panelTabOrder: ["diff", "files", "browser", "activity", "side"], panelTabsClosed: [] });
});

describe("thread layouts", () => {
  it("restores each thread's pane, tab, width, terminal and maximize", () => {
    ui().setLayoutKey("a");
    ui().togglePanel("diff");
    ui().setPanelWidth(420);
    ui().togglePanel("terminal");
    ui().setPanelMaximized(true);

    ui().setLayoutKey("b");
    expect(ui().panelOpen).toBeNull();
    expect(ui().terminalOpen).toBe(false);
    expect(ui().panelMaximized).toBe(false);
    ui().togglePanel("activity");

    ui().setLayoutKey("a");
    expect(ui()).toMatchObject({
      panelOpen: true,
      panelTab: "diff",
      panelWidth: 420,
      terminalOpen: true,
      panelMaximized: true,
    });
    ui().setLayoutKey("b");
    expect(ui().panelTab).toBe("activity");
  });

  it("hands a sent draft's layout to its thread and starts the next draft fresh", () => {
    ui().setPanelOpen(true);
    ui().adoptDraftLayout("t1");
    expect(ui().layoutKey).toBe("t1");
    expect(ui().panelOpen).toBe(true);
    ui().setLayoutKey(DRAFT_LAYOUT);
    expect(ui().panelOpen).toBeNull();
    ui().setLayoutKey("t1");
    expect(ui().panelOpen).toBe(true);
  });
});

describe("panel tab row", () => {
  it("remembers each thread's order and closed set", () => {
    ui().setLayoutKey("a");
    ui().movePanelTab("side", 0);
    ui().closePanelTab("files");
    ui().setLayoutKey("b");
    expect(ui().panelTabOrder).toEqual(["diff", "files", "browser", "activity", "side"]);
    expect(ui().panelTabsClosed).toEqual([]);
    ui().setLayoutKey("a");
    expect(ui().panelTabOrder).toEqual(["side", "diff", "files", "browser", "activity"]);
    expect(ui().panelTabsClosed).toEqual(["files"]);
  });

  it("drops stale ids from a saved row and never leaves it empty", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({ panelTab: "pulls", panelTabOrder: ["pulls", "side", "side", "diff"], panelTabsClosed: ["diff", "files", "browser", "activity", "side"] }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBeNull();
    expect(fresh.panelTabOrder).toEqual(["side", "diff", "files", "browser", "activity"]);
    expect(fresh.panelTabsClosed).toEqual([]);
  });

  it("skips a saved layout that is not an object instead of failing to load", async () => {
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ layouts: { a: null, b: 7, c: "x", d: { panelTab: "files" } } }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(Object.keys(fresh.layouts)).toEqual(["d"]);
    expect(fresh.layouts["d"]?.panelTab).toBe("files");
  });
});
