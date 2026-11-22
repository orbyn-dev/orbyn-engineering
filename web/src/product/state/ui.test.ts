import { beforeEach, describe, expect, it, vi } from "vitest";
import { DRAFT_LAYOUT, fileTabId, parseFileTab, useUi } from "./ui";

const ui = () => useUi.getState();

beforeEach(() => {
  ui().setLayoutKey(DRAFT_LAYOUT);
  useUi.setState({ layouts: {}, panelOpen: false, panelTab: null, terminalOpen: false, panelWidth: null, panelTabOrder: ["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"], panelTabsClosed: ["tools", "linked", "review", "devices", "library"] });
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
    ui().togglePanel("activity");
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
      JSON.stringify({ panelOpen: null, layouts: { a: { panelOpen: true, panelTab: "activity" } } }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi;
    expect(fresh.getState().panelOpen).toBe(false);
    fresh.getState().setLayoutKey("a");
    expect(fresh.getState().panelOpen).toBe(false);
    expect(fresh.getState().panelTab).toBe("activity");
  });
});

describe("saved preferences", () => {
  it("keeps Devices tabs, names, the last device and float as clean per-person state", async () => {
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({
      devices: {
        open: ["android:a", "android:a", 7, "ios:b"],
        active: "ios:b",
        names: { "android:a": "  Checkout flow  ", "ios:b": "", bad: 4 },
        float: { id: "ios:b", x: 99999, y: -20, width: 5000 },
      },
    }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi;
    expect(fresh.getState().devices).toEqual({
      open: ["android:a", "ios:b"],
      active: "ios:b",
      names: { "android:a": "Checkout flow" },
      float: { id: "ios:b", x: 4000, y: 0, width: 720 },
    });
    try {
      vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("private window"); });
      expect(() => fresh.getState().setPrefs({ devices: { open: ["ios:b"], active: "ios:b", names: {}, float: null } })).not.toThrow();
    } finally {
      vi.mocked(Storage.prototype.setItem).mockRestore();
    }
  });

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
    ui().reopenPanelTab("activity");
    ui().reopenPanelTab("side");
    ui().movePanelTab("side", 0);
    ui().closePanelTab("activity");
    ui().setLayoutKey("b");
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTabsClosed).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    ui().setLayoutKey("a");
    expect(ui().panelTabOrder).toEqual(["side", "diff", "browser", "activity", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTabsClosed).toEqual(["diff", "browser", "tools", "linked", "review", "devices", "library", "activity"]);
    expect(ui().panelTab).toBe("side");
  });

  it("drops stale ids from a saved row and keeps every tab closed when all were", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({ panelTab: "pulls", panelTabOrder: ["pulls", "side", "side", "diff"], panelTabsClosed: ["diff", "files", "browser", "activity", "side"] }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBeNull();
    expect(fresh.panelTabOrder).toEqual(["side", "diff", "browser", "activity", "tools", "linked", "review", "devices", "library"]);
    expect(fresh.panelTabsClosed).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
  });

  it("starts every thread's side panel with no tab open, and the order and closed set are per thread", () => {
    ui().setLayoutKey("fresh");
    expect(ui().panelTabsClosed).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    ui().reopenPanelTab("browser");
    expect(ui().panelTabsClosed).not.toContain("browser");
    ui().closeAllPanelTabs();
    expect(ui().panelTab).toBeNull();
    expect(ui().panelOpen).toBe(true);
    expect(ui().panelTabsClosed).toHaveLength(9);
  });

  it("closes the other tabs and keeps the one chosen, and Reset tab order leaves the open tabs alone", () => {
    ui().setLayoutKey("c");
    ui().reopenPanelTab("diff");
    ui().reopenPanelTab("browser");
    ui().reopenPanelTab("side");
    ui().movePanelTab("side", 0);
    ui().resetPanelTabs();
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTabsClosed).toEqual(["activity", "tools", "linked", "review", "devices", "library"]);
    ui().closeOtherPanelTabs("browser");
    expect(ui().panelTabsClosed).toEqual(["diff", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTab).toBe("browser");
  });

  it("opens a file from the left bar as a tab, once, and a closed file is gone", () => {
    ui().setLayoutKey("files");
    const target = { kind: "explorer", projectId: "p", worktreeId: "w", path: "auth/redirect.ts" } as const;
    ui().openFileTab(target);
    ui().openFileTab(target);
    const id = fileTabId(target);
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library", id]);
    expect(ui().panelTab).toBe(id);
    expect(ui().panelOpen).toBe(true);
    expect(parseFileTab(id)).toEqual(target);
    ui().closePanelTab(id);
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTabsClosed).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTab).toBeNull();
  });

  it("keeps open files per thread, behind Reset tab order and out of Close other tabs", () => {
    ui().setLayoutKey("a");
    const one = fileTabId({ kind: "thread", path: "a.ts" });
    const two = fileTabId({ kind: "thread", path: "b.ts" });
    ui().openFileTab({ kind: "thread", path: "a.ts" });
    ui().openFileTab({ kind: "thread", path: "b.ts" });
    ui().reopenPanelTab("diff");
    ui().movePanelTab(two, 0);
    ui().setLayoutKey("b");
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    ui().setLayoutKey("a");
    expect(ui().panelTabOrder).toEqual([two, "diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library", one]);
    ui().resetPanelTabs();
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library", two, one]);
    ui().closeOtherPanelTabs("diff");
    expect(ui().panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(ui().panelTab).toBe("diff");
  });

  it("keeps a file tab saved with names in its id as the same tab as a new one", async () => {
    const old = `file:${encodeURIComponent(JSON.stringify(["x", "p", "w", "a.ts", "orbyn", "main"]))}`;
    const now = fileTabId({ kind: "explorer", projectId: "p", worktreeId: "w", path: "a.ts" });
    expect(old).not.toBe(now);
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelTab: old, panelTabOrder: [old, "diff"], panelTabsClosed: [], layouts: { a: { panelTab: old, panelTabOrder: [old], panelTabsClosed: [] } } }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBe(now);
    expect(fresh.panelTabOrder.slice(0, 2)).toEqual([now, "diff"]);
    expect(fresh.layouts["a"]?.panelTab).toBe(now);
    expect(parseFileTab(old)).toEqual({ kind: "explorer", projectId: "p", worktreeId: "w", path: "a.ts" });
  });

  it("names a file of another repository by its repository too, and leaves every other tab's id as it was", async () => {
    // The id of a primary-repository or ordinary-thread tab is byte for byte what older sessions saved.
    const primary = fileTabId({ kind: "thread", path: "README.md" });
    expect(primary).toBe(`file:${encodeURIComponent(JSON.stringify(["t", "README.md"]))}`);
    const planner = fileTabId({ kind: "thread", path: "README.md", repoId: "planner" });
    const other = fileTabId({ kind: "thread", path: "README.md", repoId: "docs" });
    expect(new Set([primary, planner, other]).size).toBe(3);
    expect(parseFileTab(planner)).toEqual({ kind: "thread", path: "README.md", repoId: "planner" });
    expect(parseFileTab(primary)).toEqual({ kind: "thread", path: "README.md" });
    expect("repoId" in parseFileTab(primary)!).toBe(false);
    // Saved before repositories had tabs: opens as it did.
    const saved = `file:${encodeURIComponent(JSON.stringify(["t", "a.ts"]))}`;
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelTab: saved, panelTabOrder: ["diff", saved, planner], panelTabsClosed: [], layouts: { a: { panelTab: planner, panelTabOrder: [planner, saved], panelTabsClosed: [] } } }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBe(saved);
    expect(fresh.panelTabOrder.slice(0, 3)).toEqual(["diff", saved, planner]);
    expect(fresh.layouts["a"]?.panelTabOrder?.filter((id) => id.startsWith("file:"))).toEqual([planner, saved]);
    // An id naming an empty repository is not a tab.
    expect(parseFileTab(`file:${encodeURIComponent(JSON.stringify(["t", "a.ts", ""]))}`)).toBeNull();
    // Opening one adds one tab per repository and path.
    ui().setLayoutKey("rt");
    ui().openFileTab({ kind: "thread", path: "README.md", repoId: "planner" });
    ui().openFileTab({ kind: "thread", path: "README.md", repoId: "planner" });
    ui().openFileTab({ kind: "thread", path: "README.md" });
    expect(ui().panelTabOrder.filter((id) => id.startsWith("file:"))).toEqual([planner, primary]);
  });

  it("loads a saved file tab, and drops a saved id that names nothing we know", async () => {
    const id = fileTabId({ kind: "thread", path: "a.ts" });
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({ panelTab: id, panelTabOrder: ["diff", id, id, "files", 7], panelTabsClosed: [] }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(fresh.panelTab).toBe(id);
    expect(fresh.panelTabOrder).toEqual(["diff", id, "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    // A surface the saved row never listed starts closed.
    expect(fresh.panelTabsClosed).toEqual(["browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
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
    expect(fresh.panelTabsClosed).toEqual(["tools", "linked", "review", "devices", "library"]);
    expect(fresh.panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(fresh.layouts["a"]?.panelTabsClosed).toEqual(["tools", "linked", "review", "devices", "library"]);
  });

  it("moves a row saved in the old default order onto the new one, and keeps an order a person changed", async () => {
    localStorage.setItem(
      "orbyn.prefs.v1",
      JSON.stringify({
        panelTabOrder: ["diff", "files", "browser", "activity", "side", "tools"], panelTabsClosed: ["tools"],
        layouts: {
          a: { panelTab: "files", panelTabOrder: ["diff", "files", "browser", "activity", "side"], panelTabsClosed: [] },
          b: { panelTab: "diff", panelTabOrder: ["files", "diff", "browser", "activity", "side", "tools"], panelTabsClosed: [] },
        },
      }),
    );
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    // Nobody reordered these, so "Reset tab order" must not show for them.
    expect(fresh.panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(fresh.panelTabsClosed).toEqual(["tools", "linked", "review", "devices", "library"]);
    expect(fresh.layouts["a"]?.panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
    expect(fresh.layouts["a"]?.panelTabsClosed).toEqual(["tools", "linked", "review", "devices", "library"]);
    // The order a person changed is kept, less the Files tab the explorer replaced.
    expect(fresh.layouts["b"]?.panelTabOrder).toEqual(["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"]);
  });

  it("skips a saved layout that is not an object instead of failing to load", async () => {
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ layouts: { a: null, b: 7, c: "x", d: { panelTab: "activity" } } }));
    vi.resetModules();
    const fresh = (await import("./ui")).useUi.getState();
    expect(Object.keys(fresh.layouts)).toEqual(["d"]);
    expect(fresh.layouts["d"]?.panelTab).toBe("activity");
  });
});
