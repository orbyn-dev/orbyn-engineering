import { beforeEach, describe, expect, it, vi } from "vitest";
import { DRAFT_LAYOUT, useUi } from "./ui";

const ui = () => useUi.getState();

beforeEach(() => {
  ui().setLayoutKey(DRAFT_LAYOUT);
  useUi.setState({ layouts: {}, panelOpen: false, panelTab: null, terminalOpen: false, panelWidth: null, panelTabOrder: ["diff", "files", "browser", "activity", "side", "tools"], panelTabsClosed: ["tools"] });
});

describe("thread layouts", () => {
  it("restores each thread's tab, width, terminal and maximize; open or closed is one global choice", () => {
    ui().setLayoutKey("a");
    ui().togglePanel("diff");
    ui().setPanelWidth(420);
    ui().togglePanel("terminal");
    ui().setPanelMaximized(true);

    ui().setLayoutKey("b");
    // Opened in thread a, so it is open in thread b too; the rest is b's own.
    expect(ui().panelOpen).toBe(true);
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
    expect(ui().panelOpen).toBe(true);
    ui().setLayoutKey("t1");
    expect(ui().panelOpen).toBe(true);
  });

  it("keeps the panel closed in a thread never opened, and closing it closes it everywhere", () => {
    ui().setLayoutKey("a");
    expect(ui().panelOpen).toBe(false);
    ui().setLayoutKey("b");
    expect(ui().panelOpen).toBe(false);
    ui().revealFile("src/a.ts", null, { threadId: "b", cwd: "/w" });
    expect(ui().panelOpen).toBe(true);
    ui().setLayoutKey("c");
    expect(ui().panelOpen).toBe(true);
    ui().closePanel();
    ui().setLayoutKey("a");
    expect(ui().panelOpen).toBe(false);
  });

  it("opens for something the person triggered: a diff, a commit, a file or a page", () => {
    for (const open of [
      () => ui().revealDiff("a.ts", { threadId: "a", cwd: "/w" }),
      () => ui().revealCommit("abc123"),
      () => ui().revealFile("a.ts", null, { threadId: "a", cwd: "/w" }),
      () => ui().showInBrowser("http://localhost:3000"),
    ]) {
      ui().closePanel();
      expect(ui().panelOpen).toBe(false);
      open();
      expect(ui().panelOpen).toBe(true);
    }
  });

  it("sets the remembered choice aside on a phone-width window and does not save what happens there", () => {
    ui().setPanelOpen(true);
    ui().setPanelNarrow(true);
    expect(ui().panelOpen).toBe(false);
    ui().togglePanel("files");
    expect(ui().panelOpen).toBe(true);
    // What was saved is still the choice made on the wide window.
    expect(JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}").panelOpen).toBe(true);
    ui().closePanel();
    expect(JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}").panelOpen).toBe(true);
    ui().setPanelNarrow(false);
    expect(ui().panelOpen).toBe(true);
    expect(ui().desktopPanelOpen).toBeNull();
  });

  it("ignores an older saved per-thread open state and a saved null", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({ panelOpen: null, layouts: { a: { panelOpen: true, panelTab: "files" } } }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi;
    expect(fresh.getState().panelOpen).toBe(false);
    fresh.getState().setLayoutKey("a");
    expect(fresh.getState().panelOpen).toBe(false);
    expect(fresh.getState().panelTab).toBe("files");
  });
});

describe("saved preferences", () => {
  it("load with the removed modelFavorites field, ignore it and do not save it again", async () => {
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ modelFavorites: ["codex:x"], panelOpen: true }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi;
    expect(fresh.getState().panelOpen).toBe(true);
    expect("modelFavorites" in fresh.getState()).toBe(false);
    fresh.getState().setPanelWidth(400);
    expect(localStorage.getItem("orbyn.prefs.v1")).not.toContain("modelFavorites");
  });
});

describe("panel tab row", () => {
  it("remembers each thread's order and closed set", () => {
    ui().setLayoutKey("a");
    ui().movePanelTab("side", 0);
    ui().closePanelTab("files");
    ui().setLayoutKey("b");
    expect(ui().panelTabOrder).toEqual(["diff", "files", "browser", "activity", "side", "tools"]);
    expect(ui().panelTabsClosed).toEqual(["tools"]);
    ui().setLayoutKey("a");
    expect(ui().panelTabOrder).toEqual(["side", "diff", "files", "browser", "activity", "tools"]);
    expect(ui().panelTabsClosed).toEqual(["tools", "files"]);
  });

  it("drops stale ids from a saved row and never leaves it empty", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({ panelTab: "pulls", panelTabOrder: ["pulls", "side", "side", "diff"], panelTabsClosed: ["diff", "files", "browser", "activity", "side"] }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBeNull();
    expect(fresh.panelTabOrder).toEqual(["side", "diff", "files", "browser", "activity", "tools"]);
    expect(fresh.panelTabsClosed).toEqual([]);
  });

  it("gives a saved row from before the Tools tab existed that tab, closed, whatever its closed list was", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({
        panelTabOrder: ["diff", "files", "browser", "activity", "side"], panelTabsClosed: [],
        layouts: { a: { panelTab: "files", panelTabOrder: ["diff", "files", "browser", "activity", "side"], panelTabsClosed: [] } },
      }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTabsClosed).toEqual(["tools"]);
    expect(fresh.panelTabOrder).toEqual(["diff", "files", "browser", "activity", "side", "tools"]);
    expect(fresh.layouts["a"]?.panelTabsClosed).toEqual(["tools"]);
  });

  it("skips a saved layout that is not an object instead of failing to load", async () => {
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ layouts: { a: null, b: 7, c: "x", d: { panelTab: "files" } } }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(Object.keys(fresh.layouts)).toEqual(["d"]);
    expect(fresh.layouts["d"]?.panelTab).toBe("files");
  });
});
