import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi, type Mock } from "vitest";
import { WindowMenu } from "./WindowMenu";

type Invoke = (command: string, args?: Record<string, unknown>) => unknown;

const withBridge = (invoke?: Invoke) => {
  (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__ = invoke
    ? { invoke }
    : undefined;
};

const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const named = (label: string) => new RegExp(`^${escape(label)}`);
const exact = (label: string) => new RegExp(`^${escape(label)}$`);

type ExecDocument = Document & { execCommand?: (command: string) => boolean };

let invoke: Mock<Invoke>;
let exec: ReturnType<typeof vi.fn>;
const originalExec = (document as ExecDocument).execCommand;

beforeEach(() => {
  invoke = vi.fn();
  withBridge(invoke);
  exec = vi.fn();
  Object.defineProperty(document, "execCommand", { configurable: true, writable: true, value: exec });
});

afterEach(() => {
  withBridge();
  Object.defineProperty(document, "execCommand", { configurable: true, writable: true, value: originalExec });
  vi.restoreAllMocks();
});

const openMenu = async () => {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Menu" }));
  return { user, menu: await screen.findByRole("menu", { name: "Menu" }) };
};

it("opens the Menu popup with the four groups, closes with Escape and returns focus", async () => {
  render(<WindowMenu />);
  const trigger = screen.getByRole("button", { name: "Menu" });
  expect(trigger).toHaveAttribute("aria-haspopup", "menu");
  expect(trigger).toHaveAttribute("aria-expanded", "false");

  const { user, menu } = await openMenu();
  expect(trigger).toHaveAttribute("aria-expanded", "true");
  expect(within(menu).getByText("File")).toBeInTheDocument();
  expect(within(menu).getByText("Edit")).toBeInTheDocument();
  expect(within(menu).getByText("View")).toBeInTheDocument();
  expect(within(menu).getByText("Window")).toBeInTheDocument();
  expect(within(menu).getByRole("menuitem", { name: /^New Thread$/ })).toHaveFocus();

  await user.keyboard("{ArrowDown}");
  expect(within(menu).getByRole("menuitem", { name: named("Settings...") })).toHaveFocus();

  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  expect(trigger).toHaveFocus();
});

it("closes when the person presses outside it", async () => {
  render(<WindowMenu />);
  await openMenu();
  fireEvent.pointerDown(document.body);
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
});

it("sends every shell item's id", async () => {
  const items = [
    ["New Thread", "new-thread"],
    ["Settings...", "settings"],
    ["Check for Updates...", "check-updates"],
    ["Close Window", "close-window"],
    ["Paste as Text", "paste-text"],
    ["Toggle Sidebar", "toggle-sidebar"],
    ["Command Palette", "palette"],
    ["Reload", "reload"],
    ["Force Reload", "force-reload"],
    ["Actual Size", "zoom-reset"],
    ["Zoom In", "zoom-in"],
    ["Zoom Out", "zoom-out"],
    ["Full Screen", "fullscreen"],
    ["Minimize", "minimize"],
    ["Maximize", "maximize"],
  ] as const;
  render(<WindowMenu />);

  for (const [label, id] of items) {
    const { user, menu } = await openMenu();
    await user.click(within(menu).getByRole("menuitem", { name: named(label) }));
    expect(invoke).toHaveBeenLastCalledWith("window_menu_run", { id });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  }
});

it("runs the Edit items in the page and keeps their shortcuts", async () => {
  const items = [
    ["Undo", "undo"],
    ["Redo", "redo"],
    ["Cut", "cut"],
    ["Copy", "copy"],
    ["Paste", "paste"],
    ["Select All", "selectAll"],
  ] as const;
  render(<WindowMenu />);

  for (const [label, command] of items) {
    const { user, menu } = await openMenu();
    await user.click(within(menu).getByRole("menuitem", { name: exact(label) }));
    expect(exec).toHaveBeenLastCalledWith(command);
    expect(invoke).not.toHaveBeenCalledWith("window_menu_run", expect.anything());
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  }

  const { menu } = await openMenu();
  expect(within(menu).getByText("Ctrl+,")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+Shift+V")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+R")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+Shift+R")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+0")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+=")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+-")).toBeInTheDocument();
  expect(within(menu).getByText("Ctrl+M")).toBeInTheDocument();
});
