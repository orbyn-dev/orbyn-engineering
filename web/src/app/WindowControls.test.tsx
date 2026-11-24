import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../mock/api";
import { installMockFetch } from "../test-support";
import { TitleBar } from "./TitleBar";
import { WindowControls } from "./WindowControls";

const shell = vi.hoisted(() => ({ attached: true }));

vi.mock("../api/shell", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/shell")>();
  return {
    ...actual,
    inDesktopApp: () => shell.attached,
    readShellStatus: async () => ({ attached: shell.attached }),
    onShellChange: () => () => undefined,
  };
});

type Invoke = (command: string, args?: Record<string, unknown>) => unknown;

const withBridge = (invoke?: Invoke) => {
  (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__ = invoke
    ? { invoke }
    : undefined;
};

let mock: MockApi;

beforeEach(() => {
  mock = installMockFetch();
  shell.attached = true;
});

afterEach(() => {
  withBridge();
  shell.attached = true;
  vi.restoreAllMocks();
});

it("shows window controls only for the Windows desktop frame", async () => {
  mock.patchInfo({ platform: "windows" });
  const windows = render(<TitleBar layout="wide" navShown />);
  expect(await screen.findByRole("button", { name: "Minimize" })).toBeInTheDocument();
  expect(windows.container.querySelector(".titlebar")).toHaveAttribute("data-frame", "windows");
  expect(windows.container.querySelector(".titlebar")).not.toHaveAttribute("data-lights");
  windows.unmount();

  shell.attached = false;
  const browser = render(<TitleBar layout="wide" navShown />);
  expect(browser.container.querySelector(".titlebar")).not.toHaveAttribute("data-frame");
  expect(screen.queryByRole("button", { name: "Minimize" })).toBeNull();
  browser.unmount();

  shell.attached = true;
  mock.patchInfo({ platform: "macos" });
  const mac = render(<TitleBar layout="wide" navShown />);
  await waitFor(() => expect(mac.container.querySelector(".titlebar")).toHaveAttribute("data-lights"));
  expect(screen.queryByRole("button", { name: "Minimize" })).toBeNull();
});

it("titles each titlebar icon button, with its key on this platform", async () => {
  const platform = vi.spyOn(navigator, "platform", "get").mockReturnValue("Win32");
  const windows = render(<TitleBar layout="wide" navShown />);
  expect(await screen.findByRole("button", { name: "Hide sidebar" })).toHaveAttribute("title", "Hide sidebar (Ctrl+B)");
  expect(screen.getByRole("button", { name: "Back" })).toHaveAttribute("title", "Back (Ctrl+[)");
  expect(screen.getByRole("button", { name: "Forward" })).toHaveAttribute("title", "Forward (Ctrl+])");
  windows.unmount();

  platform.mockReturnValue("MacIntel");
  render(<TitleBar layout="wide" navShown={false} />);
  expect(await screen.findByRole("button", { name: "Show sidebar" })).toHaveAttribute("title", "Show sidebar (⌘B)");
  expect(screen.getByRole("button", { name: "Back" })).toHaveAttribute("title", "Back (⌘[)");
  expect(screen.getByRole("button", { name: "Forward" })).toHaveAttribute("title", "Forward (⌘])");
});

it("reads each tooltip shortcut from the saved keymap when a command is remapped", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("Win32");
  mock.setKeybindings({
    path: "~/.orbyn/keybindings.json",
    rules: [
      { key: "alt+b", command: "sidebar.toggle", when: null, remove: false, custom: true, default_keys: ["mod+b"] },
      { key: "alt+left", command: "navigation.back", when: "!terminalFocus", remove: false, custom: true, default_keys: ["mod+["] },
      { key: "alt+right", command: "navigation.forward", when: "!terminalFocus", remove: false, custom: true, default_keys: ["mod+]"] },
    ],
    issues: [],
    conflicts: [],
  });
  render(<TitleBar layout="wide" navShown />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Hide sidebar" })).toHaveAttribute("title", "Hide sidebar (Alt+B)"));
  expect(screen.getByRole("button", { name: "Back" })).toHaveAttribute("title", "Back (Alt+←)");
  expect(screen.getByRole("button", { name: "Forward" })).toHaveAttribute("title", "Forward (Alt+→)");
});

it("sends each window control's command to the shell", async () => {
  const invoke = vi.fn(async () => undefined);
  withBridge(invoke);
  const user = userEvent.setup();
  render(<WindowControls />);

  await user.click(screen.getByRole("button", { name: "Minimize" }));
  await user.click(screen.getByRole("button", { name: "Maximize" }));
  await user.click(screen.getByRole("button", { name: "Close" }));

  expect(invoke).toHaveBeenCalledWith("plugin:window|minimize");
  expect(invoke).toHaveBeenCalledWith("plugin:window|toggle_maximize");
  expect(invoke).toHaveBeenCalledWith("plugin:window|close");
});

it("shows Restore when the shell says the window is already maximized", async () => {
  withBridge(async (command: string) => (command === "plugin:window|is_maximized" ? true : undefined));
  render(<WindowControls />);
  expect(await screen.findByRole("button", { name: "Restore" })).toBeInTheDocument();
});

it("shows Restore while maximized and reads the state again on resize", async () => {
  let maximized = false;
  const invoke = vi.fn(async (command: string) => (command === "plugin:window|is_maximized" ? maximized : undefined));
  withBridge(invoke);
  render(<WindowControls />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("plugin:window|is_maximized"));
  expect(screen.getByRole("button", { name: "Maximize" })).toBeInTheDocument();

  maximized = true;
  fireEvent(window, new Event("resize"));
  expect(await screen.findByRole("button", { name: "Restore" })).toBeInTheDocument();

  maximized = false;
  fireEvent(window, new Event("resize"));
  expect(await screen.findByRole("button", { name: "Maximize" })).toBeInTheDocument();
});

it("sends the Maximize rect in device pixels (CSS pixels times the zoomed ratio) on mount and clears it on unmount", async () => {
  vi.stubGlobal("devicePixelRatio", 2);
  const box = { x: 12, y: 0, width: 46, height: 32, top: 0, right: 58, bottom: 32, left: 12, toJSON: () => ({}) } as DOMRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(box);
  const invoke = vi.fn(async () => false);
  withBridge(invoke);
  const controls = render(<WindowControls />);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("window_set_maximize_rect", {
      rect: { x: 24, y: 0, width: 92, height: 64 },
    }),
  );

  controls.unmount();
  expect(invoke).toHaveBeenCalledWith("window_set_maximize_rect", { rect: null });
});

it("marks the controls inactive while the window has no focus", () => {
  withBridge(async () => false);
  render(<WindowControls />);
  const controls = screen.getByRole("group", { name: "Window controls" });
  expect(controls).not.toHaveAttribute("data-inactive");
  fireEvent.blur(window);
  expect(controls).toHaveAttribute("data-inactive");
  fireEvent.focus(window);
  expect(controls).not.toHaveAttribute("data-inactive");
});
