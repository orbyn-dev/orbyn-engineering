import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { installMockFetch, setViewportWidth } from "../test-support";
import { api } from "../api/client";
import { useWorkspace } from "../product/state/workspace";
import { useUi } from "../product/state/ui";
import { runShellCommand } from "./shell/commands";
import { parseRoute } from "../product/state/route";
import { App } from "./App";
import * as threadApi from "../features/thread/api";

beforeEach(() => {
  installMockFetch();
  useUi.getState().setPrefs({ welcomeDone: true });
  useUi.setState({ panelOpen: false, terminalOpen: false, workspaceLayout: "console" });
  window.location.hash = "#/threads/t-done";
});

const wait = { timeout: 8000 };
const details = () => screen.findByRole("button", { name: "Details" }, wait);
const popover = () => screen.queryByRole("dialog", { name: "Thread details" });
const card = () => screen.queryByRole("group", { name: "Thread details" });
const panel = () => screen.queryByRole("complementary", { name: "Thread context" });

describe("the Details button and the Details popover", () => {
  it("is a button with icon and label, left of Terminal and Show side panel, pressed while the popover shows", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    const button = await details();
    const terminal = screen.getByRole("button", { name: "Terminal" });
    const side = screen.getByRole("button", { name: "Show side panel" });
    expect(button).toHaveTextContent("Details");
    expect(button.querySelector("svg")).not.toBeNull();
    expect(button.compareDocumentPosition(terminal) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(terminal.compareDocumentPosition(side) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(card()).toBeInTheDocument();
  });

  it("opens as a popover under the button while the panel is closed, and Escape closes it and returns focus", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    const { container } = render(<App />);
    const button = await details();
    await user.click(button);
    expect(container.querySelector(".details-popover")).toContainElement(card());
    expect(popover()).toContainElement(card());
    expect(panel()).toBeNull();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(card()).toBeNull());
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute("aria-pressed", "false");
  });

  it("reads Not reported on the Limits row for a Claude thread, which has no window of its own", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await details());
    const label = within(card()!).getByText("Limits");
    expect(label.closest(".drow")).toHaveTextContent("Not reported");
  });

  it("closes the popover on a press outside it", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await details());
    expect(card()).toBeInTheDocument();
    await user.click(screen.getByRole("heading", { level: 1 }));
    await waitFor(() => expect(card()).toBeNull());
  });

  it("stays a popover under the button while the side panel is open: the panel holds no card and Details is not a tab", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await user.click(await screen.findByRole("button", { name: "Show side panel" }, wait));
    expect(panel()).not.toBeNull();
    expect(within(panel()!).queryByRole("group", { name: "Thread details" })).toBeNull();
    expect(screen.queryByRole("tab", { name: "Details" })).toBeNull();
    const button = screen.getByRole("button", { name: "Details" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(container.querySelector(".details-popover")).toContainElement(card());
    expect(panel()).not.toContainElement(card());
    expect(button).toHaveAttribute("aria-pressed", "true");
    // The same button closes it.
    await user.click(button);
    await waitFor(() => expect(popover()).toBeNull());
  });

  it("stays a popover over the thread on a medium window with the overlay panel open", async () => {
    setViewportWidth(1000);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Show side panel" }, wait));
    expect(panel()).toHaveAttribute("data-mode", "overlay");
    await user.click(screen.getByRole("button", { name: "Details" }));
    expect(popover()).toBeInTheDocument();
    expect(panel()).not.toContainElement(card());
  });

  it("is a popover in Ledger too: no rail beside the thread", async () => {
    setViewportWidth(1400);
    useUi.setState({ workspaceLayout: "ledger" });
    const user = userEvent.setup();
    const { container } = render(<App />);
    expect(container.querySelector(".thread-main > .details")).toBeNull();
    expect(await details()).toHaveAttribute("aria-pressed", "false");
    await user.click(await details());
    expect(popover()).toBeInTheDocument();
  });

  it("is a sheet from the bottom with a grabber on a phone-width window", async () => {
    setViewportWidth(500);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await user.click(await details());
    expect(container.querySelector(".details-popover")).toHaveAttribute("data-sheet");
    expect(container.querySelector(".details-grab")).not.toBeNull();
  });

  it("titles the card with plain text: there is no collapse control", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await details());
    expect(within(card()!).getByText("Details")).toBeInTheDocument();
    expect(within(card()!).queryByRole("button", { name: /Collapse card|Expand card/ })).toBeNull();
  });

  it("opens only from the Details button: the side panel shortcut leaves the popover alone and toggles the panel", async () => {
    setViewportWidth(1400);
    render(<App />);
    await details();
    act(() => void runShellCommand("threadPanel.toggle", parseRoute(window.location.hash)));
    expect(popover()).toBeNull();
    await waitFor(() => expect(panel()).not.toBeNull());
  });

  it("puts the sheet away when its grabber or header is dragged down, and not for a short drag", async () => {
    setViewportWidth(500);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await user.click(await details());
    const grab = container.querySelector(".details-grab")!;
    fireEvent.pointerDown(grab, { clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(grab, { clientY: 130, pointerId: 1 });
    expect(popover()).toBeInTheDocument();
    fireEvent.pointerDown(grab, { clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(grab, { clientY: 200, pointerId: 1 });
    await waitFor(() => expect(popover()).toBeNull());
    await user.click(await details());
    const head = within(card()!).getByText("Details");
    fireEvent.pointerDown(head, { clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(head, { clientY: 180, pointerId: 1 });
    await waitFor(() => expect(popover()).toBeNull());
  });

  it("keeps its title and shows one error row with Try again for what could not be read, and the rows that loaded stay", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    const diff = vi.spyOn(api, "diff").mockRejectedValue(new Error("down"));
    vi.spyOn(threadApi, "listThreadServers").mockRejectedValue(new Error("down"));
    render(<App />);
    await user.click(await details());
    const alert = await within(card()!).findByRole("alert", {}, wait);
    expect(alert).toHaveTextContent("Couldn’t read the details.");
    // One row for all that failed, in the place of the first.
    expect(within(card()!).getAllByRole("alert")).toHaveLength(1);
    expect(within(card()!).getByText("Details")).toBeInTheDocument();
    expect(within(card()!).queryByText("None running")).toBeNull();
    expect(within(card()!).getByText("Limits")).toBeInTheDocument();
    diff.mockResolvedValue([]);
    vi.spyOn(threadApi, "listThreadServers").mockResolvedValue([]);
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(within(card()!).queryByRole("alert")).toBeNull());
    expect(within(card()!).getByText("None running")).toBeInTheDocument();
  });

  it("says so in quiet words, with no chevron, for a row with nothing to show", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    vi.spyOn(api, "diff").mockResolvedValue([]);
    vi.spyOn(threadApi, "listThreadServers").mockResolvedValue([]);
    vi.spyOn(threadApi, "listThreadScripts").mockResolvedValue({ scripts: [] } as never);
    render(<App />);
    await user.click(await details());
    expect(await within(card()!).findByText("No changes", {}, wait)).toBeInTheDocument();
    expect(within(card()!).getByText("None running")).toBeInTheDocument();
    expect(within(card()!).getByText("None in this project")).toBeInTheDocument();
    expect(within(card()!).getByRole("button", { name: /^Changes/ }).querySelector("svg:last-child:not(:first-child)")).toBeNull();
  });

  it("expands Scripts on Enter to each script with a Run button, still in the popover", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    vi.spyOn(threadApi, "listThreadScripts").mockResolvedValue({ scripts: [{ name: "dev", command: "vite --port 5173" }, { name: "lint", command: "eslint ." }] } as never);
    render(<App />);
    await user.click(await details());
    const row = await within(card()!).findByRole("button", { name: /Scripts/ }, wait);
    expect(row).toHaveAttribute("aria-expanded", "false");
    row.focus();
    await user.keyboard("{Enter}");
    expect(row).toHaveAttribute("aria-expanded", "true");
    expect(within(card()!).getByRole("button", { name: "Run dev" })).toBeInTheDocument();
    expect(within(card()!).getByRole("button", { name: "Run lint" })).toBeInTheDocument();
    expect(popover()).toBeInTheDocument();
  });

  it("opens the Rows menu on a right-click, and each row toggles at once with the menu still open", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await details());
    await user.pointer({ keys: "[MouseRight]", target: card()! });
    const usage = await screen.findByRole("menuitemcheckbox", { name: "Usage" });
    expect(usage).toHaveAttribute("aria-checked", "false");
    await user.click(usage);
    expect(screen.getByRole("menuitemcheckbox", { name: "Usage" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menu", { name: "Thread details rows" })).toBeInTheDocument();
    await user.click(screen.getByRole("menuitemcheckbox", { name: "Usage" }));
    expect(screen.getByRole("menuitemcheckbox", { name: "Usage" })).toHaveAttribute("aria-checked", "false");
  });

  it("holds only its rows; the folder path and Delete thread are in the header's thread menu", async () => {
    setViewportWidth(1400);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await details());
    expect(within(card()!).queryByText("This computer")).toBeNull();
    expect(within(card()!).queryByRole("button", { name: /Copy folder path/ })).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Thread menu" }));
    const menu = await screen.findByRole("menu", { name: "Thread menu" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Settle", "Snooze…", "Rename", "Copy folder path", "Archive", "Delete thread"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Delete thread" }));
    expect(await screen.findByRole("dialog", { name: /Delete “/ })).toBeInTheDocument();
  });

  it("shows the read-only banner for an archived thread", async () => {
    setViewportWidth(1400);
    // Archived on the service itself, so nothing the app reads again can undo it mid-test.
    await api.patchThread("t-done", { archived: true });
    render(<App />);
    expect(await screen.findByText("This thread is archived and read-only.", {}, wait)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /^Message / })).toBeNull();
  });

  it("switches the composer to the banner when the open thread is archived", async () => {
    setViewportWidth(1400);
    render(<App />);
    expect(await screen.findByRole("textbox", { name: /^Message / }, wait)).toBeInTheDocument();
    await api.patchThread("t-done", { archived: true });
    await act(async () => {
      await useWorkspace.getState().refreshThread("t-done");
    });
    expect(await screen.findByText("This thread is archived and read-only.", {}, wait)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /^Message / })).toBeNull();
  });
});
