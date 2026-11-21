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
import { runShellCommand } from "./commands";

vi.mock("../../features/panels/browser/BrowserPanel", () => ({ BrowserPanel: () => <div>Browser content</div> }));
vi.mock("../../features/panels/diff/DiffPanel", () => ({ DiffPanel: () => <div>Diff content</div> }));
vi.mock("../../features/panels/files/FileTab", () => ({ FileTab: () => <div>Files content</div> }));
vi.mock("../../features/panels/activity/ActivityPanel", () => ({ ActivityPanel: () => null }));

const thread = { id: "thread-1" } as Thread;
const threadRoute = () => parseRoute("#/threads/thread-1");

beforeEach(() => {
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
