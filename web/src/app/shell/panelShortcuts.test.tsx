import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { PanelHost } from "../../features/panels/PanelHost";
import { useConnection } from "../../product/state/connection";
import { parseRoute } from "../../product/state/route";
import { runHandlers } from "../../product/shell/shortcuts";
import { useUi } from "../../product/state/ui";
import { CLOSED_TABS_KEPT, forgetClosedTabs } from "../../product/state/ui/store";
import { fileTabId } from "../../product/state/ui/fileTab";
import { runShellCommand } from "./commands";

vi.mock("../../features/panels/browser/BrowserPanel", () => ({ BrowserPanel: () => <div>Browser content</div> }));
vi.mock("../../features/panels/diff/DiffPanel", () => ({ DiffPanel: () => <div>Diff content</div> }));
vi.mock("../../features/panels/files/FileTab", () => ({ FileTab: () => <div>Files content</div> }));
vi.mock("../../features/panels/activity/ActivityPanel", () => ({ ActivityPanel: () => null }));

const thread = { id: "thread-1" } as Thread;
const threadRoute = () => parseRoute("#/threads/thread-1");

beforeEach(() => {
  forgetClosedTabs();
  vi.spyOn(api, "diff").mockResolvedValue([]);
  vi.spyOn(api, "keybindings").mockResolvedValue({ rules: [], issues: [] } as never);
  useConnection.setState({ offline: false, serviceDown: false });
  useUi.setState({ panelOpen: true, panelTab: "diff", panelMaximized: false, panelTabOrder: ["diff", "browser", "activity", "side", "tools"], panelTabsClosed: [] });
});

describe("view.reopenClosed (mod+shift+t)", () => {
  it("brings back the tab closed last, opens the panel on it and leaves the others closed", () => {
    useUi.getState().closePanelTab("browser");
    useUi.getState().closePanelTab("activity");
    useUi.getState().closePanel();
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState()).toMatchObject({ panelOpen: true, panelTab: "activity", panelTabsClosed: ["browser"] });
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState()).toMatchObject({ panelTab: "browser", panelTabsClosed: [] });
  });

  it("does nothing when no tab is closed, and off a thread", () => {
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(false);
    useUi.getState().closePanelTab("browser");
    expect(runShellCommand("view.reopenClosed", parseRoute("#/usage"))).toBe(false);
    expect(useUi.getState().panelTabsClosed).toEqual(["browser"]);
  });
});

describe("view.reopenClosed with file tabs", () => {
  const file = (path: string) => fileTabId({ kind: "thread", path });
  const open = (...paths: string[]) => paths.forEach((path) => useUi.getState().openFileTab({ kind: "thread", path }));

  it("brings back a closed file tab on one stack with the panel's own tabs, newest first", () => {
    open("a.ts", "b.ts");
    useUi.getState().closePanelTab(file("a.ts"));
    useUi.getState().closePanelTab("browser");
    useUi.getState().closePanelTab(file("b.ts"));
    expect(useUi.getState().panelTabOrder).not.toContain(file("a.ts"));
    useUi.getState().closePanel();
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState()).toMatchObject({ panelOpen: true, panelTab: file("b.ts") });
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState()).toMatchObject({ panelTab: "browser", panelTabsClosed: [] });
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState()).toMatchObject({ panelTab: file("a.ts") });
    expect(useUi.getState().panelTabOrder).toEqual(expect.arrayContaining([file("a.ts"), file("b.ts")]));
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(false);
  });

  it("skips a file tab that was opened again by hand", () => {
    open("a.ts", "b.ts");
    useUi.getState().closePanelTab(file("a.ts"));
    useUi.getState().closePanelTab(file("b.ts"));
    open("b.ts");
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState().panelTab).toBe(file("a.ts"));
    expect(useUi.getState().panelTabOrder.filter((id) => id === file("b.ts"))).toHaveLength(1);
  });

  it("remembers what Close others and Close all closed", () => {
    open("a.ts", "b.ts");
    useUi.getState().closeOtherPanelTabs(file("b.ts"));
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(true);
    expect(useUi.getState().panelTabOrder).toContain(file("a.ts"));
  });

  it("keeps the last few closed tabs only", () => {
    const paths = Array.from({ length: CLOSED_TABS_KEPT + 5 }, (_, at) => `f${at}.ts`);
    open(...paths);
    for (const path of paths) useUi.getState().closePanelTab(file(path));
    let reopened = 0;
    while (runShellCommand("view.reopenClosed", threadRoute())) reopened += 1;
    expect(reopened).toBe(CLOSED_TABS_KEPT);
    expect(useUi.getState().panelTabOrder).not.toContain(file("f0.ts"));
    expect(useUi.getState().panelTabOrder).toContain(file(`f${CLOSED_TABS_KEPT + 4}.ts`));
  });

  it("keeps each thread's closed tabs apart", () => {
    open("a.ts");
    useUi.getState().closePanelTab(file("a.ts"));
    const before = useUi.getState().layoutKey;
    useUi.getState().setLayoutKey("thread-2");
    useUi.setState({ panelTabsClosed: [] });
    expect(runShellCommand("view.reopenClosed", threadRoute())).toBe(false);
    useUi.getState().setLayoutKey(before);
  });
});

function panel(mode: "docked" | "sheet", onToggleMaximize = vi.fn()) {
  render(<PanelHost thread={thread} mode={mode} closing={false} moves={false} maximized={false} onToggleMaximize={onToggleMaximize} onReturn={() => {}} />);
  return onToggleMaximize;
}

describe("rightPanel.new (mod+t)", () => {
  it("opens the launcher list under the plus button", async () => {
    panel("docked");
    expect(screen.queryByRole("menu", { name: "Open a tab" })).toBeNull();
    act(() => void runHandlers("rightPanel.new"));
    expect(await screen.findByRole("menu", { name: "Open a tab" })).toBeInTheDocument();
    const labels = [...document.querySelectorAll("[data-menu-root] [role=menuitem]")].map((item) => item.getAttribute("data-label"));
    expect(labels).toEqual(expect.arrayContaining(["Changes", "Browser", "Open file…"]));
  });

  it("lets a choice from the opened list open its tab", async () => {
    const user = userEvent.setup();
    useUi.setState({ panelTabsClosed: ["browser"], panelTab: "diff" });
    panel("docked");
    act(() => void runHandlers("rightPanel.new"));
    await user.click(await screen.findByRole("menuitem", { name: /Browser/ }));
    expect(useUi.getState()).toMatchObject({ panelTab: "browser", panelTabsClosed: [] });
  });

  it("has no handler once the panel is gone", () => {
    const view = render(<PanelHost thread={thread} mode="docked" closing={false} moves={false} maximized={false} onToggleMaximize={() => {}} onReturn={() => {}} />);
    view.unmount();
    expect(runHandlers("rightPanel.new")).toBe(false);
  });
});

describe("rightPanel.maximize", () => {
  it("toggles the docked panel's maximise", () => {
    const toggle = panel("docked");
    expect(runHandlers("rightPanel.maximize")).toBe(true);
    expect(toggle).toHaveBeenCalledTimes(1);
  });

  it("is unavailable in the phone's sheet, which has no maximise", () => {
    const toggle = panel("sheet");
    expect(runHandlers("rightPanel.maximize")).toBe(false);
    expect(toggle).not.toHaveBeenCalled();
  });
});
