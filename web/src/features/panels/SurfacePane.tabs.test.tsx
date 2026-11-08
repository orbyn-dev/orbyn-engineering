import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { useConnection } from "../../state/connection";
import { useUi, type RightTab } from "../../state/ui";
import { SurfacePane, type PaneMode } from "./SurfacePane";

vi.mock("./BrowserPanel", () => ({ BrowserPanel: () => <div>Browser content</div> }));
vi.mock("./DiffPanel", () => ({ DiffPanel: () => <div>Diff content</div> }));
vi.mock("./FilesPanel", () => ({ FilesPanel: () => <div>Files content</div> }));
vi.mock("./ActivityPanel", () => ({ ActivityPanel: () => null }));
vi.mock("./SideChatPane", () => ({ SideChatPane: () => <div>Side chat content</div> }));
vi.mock("./sideChat", () => ({ openSideChat: vi.fn(), closeSideChat: vi.fn() }));

const thread = { id: "thread-1" } as Thread;
const names = () => screen.getAllByRole("tab").map((tab) => tab.getAttribute("aria-label"));
/** Closes the open tab from the line under the strip. */
async function closeOpenTab(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Tab menu" }));
  await user.click(screen.getByRole("menuitem", { name: "Close tab" }));
}

beforeEach(() => {
  vi.spyOn(api, "diff").mockResolvedValue([]);
  useConnection.setState({ offline: false, serviceDown: false });
  useUi.setState({ panelOpen: true, panelTab: null, terminalOpen: false, panelTabOrder: ["diff", "files", "browser", "activity", "side", "tools"], panelTabsClosed: ["tools"] });
});

function pane(props: { mode?: PaneMode; onReturn?: () => void; onToggleMaximize?: () => void; maximized?: boolean } = {}) {
  return render(
    <SurfacePane
      thread={thread}
      mode={props.mode ?? "docked"}
      closing={false}
      moves={false}
      maximized={props.maximized ?? false}
      onToggleMaximize={props.onToggleMaximize ?? (() => {})}
      onReturn={props.onReturn ?? (() => {})}
    />,
  );
}

describe("surface tabs", () => {
  it("shows the five surfaces as icon-only slots, with no Pull requests tab and no Terminal tab, and the open one's name under the strip", () => {
    pane();
    expect(names()).toEqual(["Diff", "Files", "Browser", "Activity", "Side chat"]);
    for (const tab of screen.getAllByRole("tab")) expect(tab).toHaveTextContent("");
    expect(screen.queryByRole("button", { name: /^Close .* tab$/ })).not.toBeInTheDocument();
    const line = screen.getByRole("button", { name: "Tab menu" }).closest(".ctx-title")!;
    expect(line.querySelector("b")).toHaveTextContent("Diff");
  });

  it("names the open tab on the line under the strip, and shows a tab's name on its slot for the tooltip", async () => {
    const user = userEvent.setup();
    pane();
    await user.click(screen.getByRole("tab", { name: "Browser" }));
    expect(screen.getByRole("button", { name: "Tab menu" }).closest(".ctx-title")!.querySelector("b")).toHaveTextContent("Browser");
    expect(screen.getByRole("tab", { name: "Files" })).toHaveAttribute("data-tip", "Files");
    expect(screen.getByRole("button", { name: "Open a tab" })).toHaveAttribute("data-tip", "Open a tab");
  });

  it("slots the Tools tab in last once it is open, as one more icon-only slot", () => {
    useUi.setState({ panelTabsClosed: [] });
    pane();
    expect(names()).toEqual(["Diff", "Files", "Browser", "Activity", "Side chat", "Tools"]);
    expect(screen.getByRole("tab", { name: "Tools" })).toHaveAttribute("data-tip", "Tools");
  });

  it("shows the first surface until another is chosen, and keeps the chosen one when its tab is pressed again", async () => {
    const user = userEvent.setup();
    pane();
    expect(screen.getByText("Diff content")).toBeInTheDocument();
    expect(screen.queryByLabelText("Open a surface")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Files" }));
    expect(screen.getByText("Files content")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Files" }));
    expect(screen.getByText("Files content")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Side chat" }));
    expect(screen.getByText("Side chat content")).toBeInTheDocument();
    expect(useUi.getState().panelTab).toBe("side");
  });

  it("keeps the layout controls: maximise and close", async () => {
    const user = userEvent.setup();
    const onReturn = vi.fn();
    const onToggleMaximize = vi.fn();
    pane({ onReturn, onToggleMaximize });
    await user.click(screen.getByRole("button", { name: "Maximise panel" }));
    expect(onToggleMaximize).toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Terminal" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close side panel" }));
    expect(onReturn).toHaveBeenCalled();
  });

  it("is a full-screen sheet on the phone: Back to thread and Close, with nothing to maximise", () => {
    pane({ mode: "sheet" });
    expect(screen.getByRole("button", { name: "Back to thread" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close side panel" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Maximise panel" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("tab")).toHaveLength(5);
  });

  it("keeps the tab row, with Maximise and Close at its right end, when it floats over the thread", () => {
    pane({ mode: "overlay" });
    expect(screen.queryByRole("button", { name: "Back to thread" })).not.toBeInTheDocument();
    const row = screen.getByRole("tablist").closest("header")!;
    const buttons = [...row.querySelectorAll("button:not([role=tab])")].map((button) => button.getAttribute("aria-label"));
    expect(buttons.slice(-2)).toEqual(["Maximise panel", "Close side panel"]);
  });

  it("heads the reading layout's drawer with Back to thread at the left and Maximise and Close at the right", async () => {
    const user = userEvent.setup();
    const onReturn = vi.fn();
    useUi.setState({ workspaceLayout: "reading" });
    try {
      pane({ onReturn });
      const row = screen.getByRole("button", { name: "Back to thread" }).closest("header")!;
      expect(row.nextElementSibling?.querySelector('[role="tablist"]')).not.toBeNull();
      expect(screen.getAllByRole("button", { name: "Close side panel" })).toHaveLength(1);
      await user.click(screen.getByRole("button", { name: "Back to thread" }));
      expect(onReturn).toHaveBeenCalled();
    } finally {
      useUi.setState({ workspaceLayout: "console" });
    }
  });

  it("is one tablist: one Tab stop, arrow keys, Home and End, and a panel for the chosen tab only", async () => {
    const user = userEvent.setup();
    pane();
    const tabs = () => screen.getAllByRole("tab");
    expect(tabs().filter((tab) => tab.tabIndex === 0)).toHaveLength(1);
    await user.click(screen.getByRole("tab", { name: "Files" }));
    expect(tabs().filter((tab) => tab.tabIndex === 0)).toEqual([screen.getByRole("tab", { name: "Files" })]);
    expect(tabs().filter((tab) => tab.hasAttribute("aria-controls"))).toHaveLength(1);
    expect(document.getElementById("context-files")).not.toBeNull();
    screen.getByRole("tab", { name: "Files" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(useUi.getState().panelTab).toBe("browser");
    await user.keyboard("{End}");
    expect(useUi.getState().panelTab).toBe("side");
    await user.keyboard("{ArrowRight}");
    expect(useUi.getState().panelTab).toBe("diff");
    await user.keyboard("{ArrowLeft}");
    expect(useUi.getState().panelTab).toBe("side");
    await user.keyboard("{Home}");
    expect(useUi.getState().panelTab).toBe("diff");
  });

  it("always draws the plus; Tools starts closed there, and once opened the menu offers only Reset tab order", async () => {
    const user = userEvent.setup();
    pane();
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Tools"]);
    expect(screen.queryByRole("menuitem", { name: "Reset tab order" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Tools" }));
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Reset tab order"]);
  });

  it("moves focus to the neighbouring tab after Close tab, and announces it", async () => {
    const user = userEvent.setup();
    useUi.setState({ panelTab: "files" });
    pane();
    await closeOpenTab(user);
    expect(screen.getByRole("tab", { name: "Browser" })).toHaveFocus();
    expect(screen.getByRole("status")).toHaveTextContent("Tab closed");
  });

  it("closes with Backspace as well as Delete and keeps focus in the row", async () => {
    const user = userEvent.setup();
    pane();
    screen.getByRole("tab", { name: "Side chat" }).focus();
    await user.keyboard("{Backspace}");
    expect(useUi.getState().panelTabsClosed).toEqual(["tools", "side"]);
    expect(screen.getByRole("tab", { name: "Activity" })).toHaveFocus();
  });

  it("closes the open tab from its menu, opens the next, and brings it back from the plus with Reset tab order under it", async () => {
    const user = userEvent.setup();
    useUi.setState({ panelTab: "files" });
    pane();
    await closeOpenTab(user);
    expect(screen.queryByRole("tab", { name: "Files" })).not.toBeInTheDocument();
    expect(useUi.getState().panelTab).toBe("browser");
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Files", "Tools", "Reset tab order"]);
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(screen.getByRole("tab", { name: "Files" })).toBeInTheDocument();
    expect(useUi.getState().panelTab).toBe("files");
    expect(useUi.getState().panelTabsClosed).toEqual(["tools"]);
  });

  it("keeps the last tab: its menu cannot close it", async () => {
    const user = userEvent.setup();
    useUi.setState({ panelTabsClosed: ["files", "browser", "activity", "side", "tools"] });
    pane();
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Tab menu" }));
    expect(screen.getByRole("menuitem", { name: "Close tab" })).toBeDisabled();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Files", "Browser", "Activity", "Side chat", "Tools", "Reset tab order"]);
  });

  it("reorders by drag, and Reset tab order puts the default back", async () => {
    const user = userEvent.setup();
    pane();
    const tab = (name: string) => screen.getByRole("tab", { name });
    fireEvent.dragStart(tab("Side chat"));
    fireEvent.dragOver(tab("Files"));
    fireEvent.drop(tab("Files"));
    expect(names()).toEqual(["Diff", "Side chat", "Files", "Browser", "Activity"]);
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    await user.click(screen.getByRole("menuitem", { name: "Reset tab order" }));
    expect(names()).toEqual(["Diff", "Files", "Browser", "Activity", "Side chat"]);
  });

  it("moves the focused tab with Alt+Shift+Arrow and closes it with Delete", async () => {
    const user = userEvent.setup();
    pane();
    screen.getByRole("tab", { name: "Files" }).focus();
    await user.keyboard("{Alt>}{Shift>}{ArrowRight}{/Shift}{/Alt}");
    expect(names()).toEqual(["Diff", "Browser", "Files", "Activity", "Side chat"]);
    expect(useUi.getState().panelTab).toBeNull();
    screen.getByRole("tab", { name: "Files" }).focus();
    await user.keyboard("{Delete}");
    expect(screen.queryByRole("tab", { name: "Files" })).not.toBeInTheDocument();
    expect(useUi.getState().panelTabsClosed).toEqual(["tools", "files"]);
  });

  it("opens a closed surface when something reveals it", () => {
    useUi.setState({ panelTabsClosed: ["browser"] });
    useUi.getState().showInBrowser("https://example.com");
    expect(useUi.getState().panelTabsClosed).toEqual([]);
    expect(useUi.getState().panelTab).toBe("browser");
  });
});

describe("connection states in the panel", () => {
  afterEach(() => useConnection.setState({ offline: false, serviceDown: false }));

  it("marks only the tabs that need the network when offline, and says so over their content", () => {
    useConnection.setState({ offline: true });
    useUi.setState({ panelTab: "side" });
    pane();
    const offline = screen.getAllByRole("tab").filter((tab) => tab.hasAttribute("data-offline"));
    expect(offline.map((tab) => tab.id)).toEqual(["context-tab-browser", "context-tab-side"]);
    expect(screen.getByText("You’re offline.")).toBeInTheDocument();
  });

  it("says Diff is waiting for the Orbyn service when it does not answer", () => {
    useConnection.setState({ serviceDown: true });
    useUi.setState({ panelTab: "diff" });
    pane();
    expect(screen.getByText("Waiting for the Orbyn service")).toBeInTheDocument();
    expect(screen.getByText("The diff comes back when the service does.")).toBeInTheDocument();
    expect(screen.queryByText("Diff content")).not.toBeInTheDocument();
  });

  it("names the maximise button Restore once the panel is maximised", () => {
    pane({ maximized: true });
    expect(screen.getByRole("button", { name: "Restore" })).toHaveAttribute("title", "Restore");
    expect(screen.queryByRole("button", { name: "Maximise panel" })).not.toBeInTheDocument();
  });

  it("tells each tab what is waiting while the service is away, and leaves the Browser usable", () => {
    useConnection.setState({ serviceDown: true });
    const lines: [RightTab, string][] = [
      ["files", "Files open again as soon as it is back."],
      ["activity", "Activity shows again as soon as it is back."],
      ["side", "Your side chats open again as soon as it is back. Nothing is lost."],
    ];
    for (const [tab, line] of lines) {
      useUi.setState({ panelTab: tab });
      const { unmount } = pane();
      expect(screen.getByText("Waiting for the Orbyn service")).toBeInTheDocument();
      expect(screen.getByText(line)).toBeInTheDocument();
      unmount();
    }
    useUi.setState({ panelTab: "browser" });
    pane();
    expect(screen.getByText("Browser content")).toBeInTheDocument();
    expect(screen.queryByText("Waiting for the Orbyn service")).not.toBeInTheDocument();
  });

  it("marks the Browser tab offline and leaves the note to the Browser, while Diff, Files and Activity carry no mark", () => {
    useConnection.setState({ offline: true });
    useUi.setState({ panelTab: "browser" });
    pane();
    expect(screen.getByRole("tab", { name: "Browser" })).toHaveAttribute("data-offline");
    for (const name of ["Diff", "Files", "Activity"]) expect(screen.getByRole("tab", { name })).not.toHaveAttribute("data-offline");
    expect(screen.getByText("Browser content")).toBeInTheDocument();
    expect(screen.queryByText("You’re offline.")).not.toBeInTheDocument();
  });
});

describe("tabs that do not fit", () => {
  /** A strip of this width: the end buttons take 58px, so 244px leaves room for three slots, the "..." and the "+". */
  function narrow(width: number) {
    const offset = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("ctx-end") ? 58 : 0;
    });
    const client = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("ctx-strip") ? width : 0;
    });
    return () => {
      offset.mockRestore();
      client.mockRestore();
    };
  }
  const ids = () => screen.getAllByRole("tab").map((tab) => tab.id.replace("context-tab-", ""));
  let restore = () => {};
  afterEach(() => restore());

  it("folds the last tabs into a ... menu that names them, and keeps the open tab visible", async () => {
    const user = userEvent.setup();
    restore = narrow(244);
    useUi.setState({ panelTab: "side" });
    pane();
    expect(ids()).toEqual(["diff", "files", "side"]);
    await user.click(screen.getByRole("button", { name: "More tabs" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Browser", "Activity"]);
  });

  it("puts a tab chosen from the menu in the last visible slot, and leaves the other slots where they were", async () => {
    const user = userEvent.setup();
    restore = narrow(244);
    pane();
    expect(ids()).toEqual(["diff", "files", "browser"]);
    await user.click(screen.getByRole("button", { name: "More tabs" }));
    await user.click(screen.getByRole("menuitem", { name: "Side chat" }));
    expect(useUi.getState().panelTab).toBe("side");
    expect(ids()).toEqual(["diff", "files", "side"]);
    // Activating a visible tab afterwards moves nothing.
    await user.click(screen.getByRole("tab", { name: "Diff" }));
    expect(ids()).toEqual(["diff", "files", "side"]);
    await user.click(screen.getByRole("button", { name: "More tabs" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Browser", "Activity"]);
  });

  it("closes the menu with Escape and returns focus to the ... button", async () => {
    const user = userEvent.setup();
    restore = narrow(244);
    pane();
    await user.click(screen.getByRole("button", { name: "More tabs" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "More tabs" })).toHaveFocus();
  });

  it("walks the arrow keys over the visible tabs only", async () => {
    const user = userEvent.setup();
    restore = narrow(244);
    pane();
    screen.getByRole("tab", { name: "Diff" }).focus();
    await user.keyboard("{End}");
    expect(useUi.getState().panelTab).toBe("browser");
    await user.keyboard("{ArrowRight}");
    expect(useUi.getState().panelTab).toBe("diff");
  });

  it("draws no ... button when everything fits", () => {
    restore = narrow(400);
    pane();
    expect(ids()).toHaveLength(5);
    expect(screen.queryByRole("button", { name: "More tabs" })).not.toBeInTheDocument();
  });
});
