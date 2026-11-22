import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { installMockFetch, setViewportWidth } from "../test-support";
import { useUi } from "../product/state/ui";
import { navigate } from "../product/state/route";
import { App } from "./App";
import { useSidebarDrawer } from "../product/shell/sidebarDrawer";

beforeEach(() => {
  installMockFetch();
  useUi.getState().setPrefs({ welcomeDone: true });
  useUi.setState({ panelOpen: false, terminalOpen: false });
  useSidebarDrawer.setState({ open: false });
  window.location.hash = "#/threads/t-done";
});

const panel = () => screen.queryByRole("complementary", { name: "Thread context" });
const sidebar = () => screen.queryByRole("navigation", { name: "Workspace" });
const thread = () => document.querySelector(".thread-main") as HTMLElement | null;
// The first render of the whole app can take longer than the default wait on a busy machine.
const showPanel = () => screen.findByRole("button", { name: "Show side panel" }, { timeout: 8000 });

describe("layouts by window width", () => {
  it("docks the sidebar, the thread and the panel from 1160 px", async () => {
    setViewportWidth(1160);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await showPanel());
    expect(sidebar()).toBeInTheDocument();
    expect(panel()).toHaveAttribute("data-mode", "docked");
    expect(thread()).not.toHaveAttribute("hidden");
  });

  it("keeps the sidebar docked from 900 to 1159 px and floats the panel over the thread", async () => {
    setViewportWidth(1159);
    const user = userEvent.setup();
    render(<App />);
    const button = await showPanel();
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(sidebar()).toBeInTheDocument();
    expect(panel()).toHaveAttribute("data-mode", "overlay");
    // Non-modal: the thread stays where it was, mounted and not hidden.
    expect(thread()).not.toHaveAttribute("hidden");
    expect(button).toHaveAttribute("aria-pressed", "true");
  });

  it("closes the overlay with Escape and returns focus to the header button, or with the button itself", async () => {
    setViewportWidth(1000);
    const user = userEvent.setup();
    render(<App />);
    const button = await showPanel();
    await user.click(button);
    await user.click(screen.getByRole("button", { name: "Open a tab" }));
    await user.click(screen.getByRole("menuitem", { name: "Tools" }));
    expect(panel()).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(useUi.getState().panelOpen).toBe(false));
    expect(button).toHaveFocus();
    await user.click(button);
    expect(useUi.getState().panelOpen).toBe(true);
    await user.click(button);
    expect(useUi.getState().panelOpen).toBe(false);
  });

  it("folds the sidebar into a drawer from 600 to 899 px, opened from the title bar over a scrim", async () => {
    setViewportWidth(899);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await showPanel();
    expect(sidebar()).not.toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "Show sidebar" });
    await user.click(toggle);
    expect(sidebar()).toBeInTheDocument();
    expect(container.querySelector(".drawer-scrim")).not.toBeNull();
    expect(container.querySelector(".shell")).toHaveAttribute("data-drawer", "open");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    expect(toggle).toHaveFocus();
  });

  it("closes the drawer on the scrim and on choosing a thread", async () => {
    setViewportWidth(720);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await showPanel();
    await user.click(screen.getByRole("button", { name: "Show sidebar" }));
    await user.click(container.querySelector(".drawer-scrim")!);
    expect(sidebar()).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show sidebar" }));
    const row = await within(sidebar()!).findByText("Upgrade tokio and fix the fallout");
    await user.click(row);
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    expect(window.location.hash).not.toBe("#/threads/t-done");
  });

  it("opens the drawer or the panel, never both", async () => {
    setViewportWidth(800);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await showPanel());
    expect(panel()).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show sidebar" }));
    expect(sidebar()).toBeInTheDocument();
    expect(useUi.getState().panelOpen).toBe(false);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Show side panel" }));
    expect(useUi.getState().panelOpen).toBe(true);
    await user.click(screen.getByRole("button", { name: "Show side panel" }));
    act(() => useSidebarDrawer.getState().setOpen(true));
    await user.click(screen.getByRole("button", { name: "Show side panel" }));
    expect(useUi.getState().panelOpen).toBe(true);
    expect(useSidebarDrawer.getState().open).toBe(false);
  });

  it("is the phone layout under 600 px: the panel is a sheet that replaces the thread", async () => {
    setViewportWidth(599);
    useUi.setState({ layoutKey: "t-done", panelOpen: false, panelTab: "activity" });
    render(<App />);
    // A phone-width window does not take the remembered choice; the person opens the sheet.
    await waitFor(() => expect(thread()).not.toBeNull(), { timeout: 8000 });
    act(() => useUi.getState().togglePanel("activity"));
    await screen.findByRole("button", { name: "Back to thread" }, { timeout: 8000 });
    expect(panel()).toHaveAttribute("data-mode", "sheet");
    expect(thread()).toHaveAttribute("hidden");
    expect(screen.queryByRole("button", { name: "Show side panel" })).not.toBeInTheDocument();
  });
});

describe("the phone's sidebar", () => {
  const sheetClose = () => screen.queryByRole("button", { name: "Close" });

  it("opens the sidebar sheet from a thread, where the sidebar is not the page", async () => {
    setViewportWidth(375);
    const user = userEvent.setup();
    const { container } = render(<App />);
    // The page is the thread, so the button says what it will do; it used to say Hide sidebar and do nothing.
    const toggle = await screen.findByRole("button", { name: "Show sidebar" }, { timeout: 8000 });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(sidebar()).not.toBeInTheDocument();
    await user.click(toggle);
    // A full-screen sheet in the page's place: the places in the sidebar are reachable from the thread.
    expect(sidebar()).toBeInTheDocument();
    expect(within(sidebar()!).getByRole("link", { name: "Pull requests" })).toBeInTheDocument();
    expect(within(sidebar()!).getByRole("link", { name: "Usage" })).toBeInTheDocument();
    expect(container.querySelector(".sidebar-sheet")).not.toBeNull();
    expect(container.querySelector(".shell")).toHaveAttribute("data-drawer", "open");
    expect(container.querySelector("main")).toHaveAttribute("hidden");
    // Focus lands in the sheet, on the link back to the page.
    expect(sheetClose()).toHaveFocus();
    expect(screen.getByRole("button", { name: "Hide sidebar" })).toHaveAttribute("aria-expanded", "true");
  });

  it("closes the sheet on Escape and on a chosen destination, leaving no focus or scroll behind", async () => {
    setViewportWidth(375);
    const user = userEvent.setup();
    const { container } = render(<App />);
    const toggle = await screen.findByRole("button", { name: "Show sidebar" }, { timeout: 8000 });
    await user.click(toggle);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    // Focus comes back to the button that stays on screen, not to the sheet that left.
    expect(toggle).toHaveFocus();
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    // Nothing is left inert or scrolled-locked behind the closed sheet: the page is back and scrolls.
    expect(container.querySelector("main")).not.toHaveAttribute("hidden");
    expect(document.body.style.overflow).toBe("");
    expect(document.documentElement.style.overflow).toBe("");
    // A destination in the sheet is a choice: it closes the sheet and changes the page.
    await user.click(screen.getByRole("button", { name: "Show sidebar" }));
    await user.click(within(sidebar()!).getByRole("link", { name: "Pull requests" }));
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    expect(container.querySelector(".shell")).not.toHaveAttribute("data-drawer");
    await waitFor(() => expect(window.location.hash).toBe("#/pulls"));
  });

  it("closes the sheet with its own Close link", async () => {
    setViewportWidth(375);
    const user = userEvent.setup();
    const { container } = render(<App />);
    await user.click(await screen.findByRole("button", { name: "Show sidebar" }, { timeout: 8000 }));
    await user.click(sheetClose()!);
    expect(sidebar()).not.toBeInTheDocument();
    expect(container.querySelector("main")).not.toHaveAttribute("hidden");
    expect(screen.getByRole("button", { name: "Show sidebar" })).toHaveFocus();
  });

  it("keeps the sidebar as the home page, where the button says Hide sidebar", async () => {
    setViewportWidth(375);
    const user = userEvent.setup();
    render(<App />);
    act(() => navigate({ name: "home" }));
    // On the home screen the sidebar is the page itself, so the button reports it as shown.
    const toggle = await screen.findByRole("button", { name: "Hide sidebar" }, { timeout: 8000 });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(sidebar()).toBeInTheDocument();
    expect(document.querySelector(".sidebar-sheet")).toBeNull();
    await user.click(toggle);
    // Leaving the sidebar's page closes it: the composer takes the page over.
    await waitFor(() => expect(sidebar()).not.toBeInTheDocument());
    await screen.findByRole("button", { name: "Show sidebar" }, { timeout: 8000 });
  });
});
