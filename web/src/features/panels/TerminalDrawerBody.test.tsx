import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { request } from "../../api/client";
import type { TerminalInfo } from "../../api/types";
import { useConnection } from "../../state/connection";
import { useUi } from "../../state/ui";
import { useComposerChips } from "../thread/composerChipStore";
import { TerminalBar, TerminalDrawerBody } from "./TerminalDrawerBody";
import { registerTerminal, terminalKey } from "./terminalControls";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId, autoFocus }: { terminalId?: string; autoFocus?: boolean }) => <div data-autofocus={autoFocus ? "yes" : "no"}>Output of {terminalId ?? "main"}<textarea className="xterm-helper-textarea" aria-label="xterm" /></div> }));
vi.mock("../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));
vi.mock("../thread/runProjectAction", () => ({ runProjectAction: vi.fn(() => Promise.resolve()) }));

const mock = vi.mocked(request);

const terminal = (over: Partial<TerminalInfo>): TerminalInfo => ({
  id: "t2",
  thread_id: "th",
  title: "npm test",
  group: "g2",
  direction: null,
  alive: true,
  exit_code: null,
  clients: 1,
  script: "test",
  created_at: new Date(Date.now() - 62_000).toISOString(),
  ...over,
});

function serve(terminals: TerminalInfo[]) {
  mock.mockImplementation((method: string, path: string) => {
    if (method === "GET" && path.endsWith("/terminals")) return Promise.resolve(terminals) as never;
    if (method === "GET" && path.endsWith("/scripts")) return Promise.resolve({ scripts: [{ name: "test", icon: "test", command: "npm test" }] }) as never;
    return Promise.resolve(undefined) as never;
  });
}

beforeEach(() => {
  mock.mockReset();
  useComposerChips.setState({ chips: {} });
  useUi.setState({ terminalOpen: true, toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
});
afterEach(() => vi.restoreAllMocks());

const show = async (terminals: TerminalInfo[]) => {
  serve(terminals);
  const user = userEvent.setup();
  render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
  await user.click(await screen.findByRole("tab", { name: /npm test/ }));
  return user;
};

describe("terminal drawer", () => {
  it("marks a tab by how its process is doing and closes it from its own button", async () => {
    const user = await show([terminal({}), terminal({ id: "t3", group: "g3", title: "build", alive: false, exit_code: 0 }), terminal({ id: "t4", group: "g4", title: "lint", alive: false, exit_code: 2 })]);
    expect(screen.getAllByLabelText("Working")).not.toHaveLength(0);
    expect(screen.getByLabelText("Finished")).toBeInTheDocument();
    expect(screen.getByLabelText("Failed")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close build" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("DELETE", "/api/threads/th/terminals/t3"));
  });

  it("shows how long a running process has gone, and clears or interrupts it", async () => {
    const clear = vi.fn();
    const interrupt = vi.fn();
    const stop = registerTerminal(terminalKey("th", "t2"), { clear, interrupt, output: () => "", find: vi.fn(), onResults: () => () => undefined });
    const user = await show([terminal({})]);
    expect(screen.getByText(/^Running · 1m 0\ds$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(clear).toHaveBeenCalled();
    expect(interrupt).toHaveBeenCalled();
    stop();
  });

  it("says a command failed with its code, sends the output to the agent and runs it again", async () => {
    const stop = registerTerminal(terminalKey("th", "t2"), { clear: vi.fn(), interrupt: vi.fn(), output: () => "1 failed | 47 passed", find: vi.fn(), onResults: () => () => undefined });
    const user = await show([terminal({ alive: false, exit_code: 1 })]);
    expect(screen.getByText("Exited with code 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Send to the agent" }));
    expect(useComposerChips.getState().chips["th"]).toEqual([expect.objectContaining({ kind: "terminal", output: "1 failed | 47 passed", exitCode: 1 })]);
    await user.click(screen.getByRole("button", { name: "Run again" }));
    const { runProjectAction } = await import("../thread/runProjectAction");
    expect(runProjectAction).toHaveBeenCalledWith("th", expect.objectContaining({ name: "test" }));
    stop();
  });

  it("keeps the output of a process that ended and offers Close tab", async () => {
    const user = await show([terminal({ alive: false, exit_code: 0 })]);
    expect(screen.getByText("Ended")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close tab" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("DELETE", "/api/threads/th/terminals/t2"));
  });

  it("offers Clear, Copy and Watch from a tab's right-click menu, and End process only while it runs; splits are not offered", async () => {
    const user = await show([terminal({})]);
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: /npm test/ }) });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Clear output", "Copy all output", "Watch only", "End process"]);
  });

  it("disables Clear and Copy for a tab that is not showing, and never says Output copied when nothing was copied", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const user = await show([terminal({}), terminal({ id: "t3", group: "g3", title: "build", alive: false, exit_code: 0 })]);
    const stop = registerTerminal(terminalKey("th", "t2"), { clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: "build" }) });
    expect(screen.getByRole("menuitem", { name: "Clear output" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Copy all output" })).toBeDisabled();
    await user.keyboard("{Escape}");
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: /npm test/ }) });
    await user.click(screen.getByRole("menuitem", { name: "Copy all output" }));
    expect(writeText).not.toHaveBeenCalled();
    expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Nothing to copy"]);
    stop();
  });

  it("opens the tab menu from the keyboard with the menu key", async () => {
    const user = await show([terminal({})]);
    screen.getByRole("tab", { name: /npm test/ }).focus();
    await user.keyboard("{ContextMenu}");
    expect(screen.getByRole("menu", { name: "Terminal tab" })).toBeInTheDocument();
  });

  it("hides Stop in Watch only mode", async () => {
    const user = await show([terminal({})]);
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Watch only" }));
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
  });

  it("asks before ending a running terminal, with Cancel focused, and closes an idle one without asking", async () => {
    const user = await show([terminal({}), terminal({ id: "t3", group: "g3", title: "build", alive: false, exit_code: 0 })]);
    await user.click(screen.getByRole("button", { name: "Close npm test" }));
    const dialog = await screen.findByRole("dialog", { name: "End this terminal?" });
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(mock).not.toHaveBeenCalledWith("DELETE", expect.anything());
    await user.click(within(dialog).getByRole("button", { name: "End" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("DELETE", "/api/threads/th/terminals/t2"));
    await user.click(screen.getByRole("button", { name: "Close build" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("DELETE", "/api/threads/th/terminals/t3"));
  });

  it("has Maximise and Collapse at the end of the row, a plus that opens a terminal, and no Close terminal or splits", async () => {
    const onClose = vi.fn();
    const onToggleMaximize = vi.fn();
    serve([terminal({})]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={onClose} onToggleMaximize={onToggleMaximize} />);
    await screen.findByRole("tab", { name: /npm test/ });
    await user.click(screen.getByRole("button", { name: "Maximise terminal" }));
    await user.click(screen.getByRole("button", { name: "Collapse terminal" }));
    expect(onToggleMaximize).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Close terminal" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Split terminal/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("POST", "/api/threads/th/terminals", {}));
  });

  it("says No terminals open once every tab is closed, and offers New terminal", async () => {
    serve([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close Terminal" }));
    expect(screen.getByText("No terminals open.")).toBeInTheDocument();
    await user.click(within(screen.getByText("No terminals open.").parentElement!).getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(mock).toHaveBeenCalledWith("POST", "/api/threads/th/terminals", {}));
  });

  it("searches the output: Ctrl+F opens the field, Enter moves to the next match, and the count says where you are", async () => {
    const find = vi.fn();
    const stop = registerTerminal(terminalKey("th", undefined), { clear: vi.fn(), interrupt: vi.fn(), output: () => "", find, onResults: (listener) => { listener({ index: 1, count: 3 }); return () => undefined; } });
    serve([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await screen.findByRole("tab", { name: "Terminal" });
    screen.getByRole("tab", { name: "Terminal" }).focus();
    await user.keyboard("{Control>}f{/Control}");
    const field = await screen.findByRole("textbox", { name: "Search the terminal" });
    await user.type(field, "Expected");
    expect(find).toHaveBeenLastCalledWith("Expected", "next");
    expect(screen.getByText("2 of 3")).toBeInTheDocument();
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(find).toHaveBeenLastCalledWith("Expected", "previous");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: "Search the terminal" })).not.toBeInTheDocument();
    stop();
  });
});

describe("terminal focus, search keys and the hidden main shell", () => {
  /** A list that forgets a terminal once it has been deleted, as the service does. */
  function serveLive(initial: TerminalInfo[]) {
    let list = initial;
    mock.mockImplementation((method: string, path: string) => {
      if (method === "DELETE") {
        const id = path.split("/").pop();
        list = list.filter((entry) => entry.id !== id);
        return Promise.resolve(undefined) as never;
      }
      if (method === "GET" && path.endsWith("/terminals")) return Promise.resolve(list) as never;
      if (method === "GET" && path.endsWith("/scripts")) return Promise.resolve({ scripts: [] }) as never;
      return Promise.resolve(undefined) as never;
    });
  }
  const idle = (id: string, title: string) => terminal({ id, group: `g-${id}`, title, alive: false, exit_code: 0 });

  it("moves focus to the neighbouring tab after a terminal tab is closed", async () => {
    serveLive([idle("t2", "build"), idle("t3", "lint")]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close build" }));
    await waitFor(() => expect(screen.queryByRole("tab", { name: "build" })).not.toBeInTheDocument());
    expect(screen.getByRole("tab", { name: "lint" })).toHaveFocus();
  });

  it("moves focus to the neighbouring tab after End is confirmed", async () => {
    serveLive([terminal({}), idle("t3", "lint")]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close npm test" }));
    await user.click(within(await screen.findByRole("dialog", { name: "End this terminal?" })).getByRole("button", { name: "End" }));
    await waitFor(() => expect(screen.queryByRole("tab", { name: /npm test/ })).not.toBeInTheDocument());
    expect(screen.getByRole("tab", { name: "lint" })).toHaveFocus();
  });

  it("hides the main shell's tab without a confirm that promises to stop it", async () => {
    serveLive([terminal({ id: "main", group: "main", title: "Terminal", script: "dev" })]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close Terminal" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Terminal" })).not.toBeInTheDocument();
    expect(mock).not.toHaveBeenCalledWith("DELETE", expect.anything());
  });

  it("asks a terminal opened here to take focus when it loads, and not one that was already there", async () => {
    let list = [idle("t2", "build")];
    mock.mockImplementation((method: string, path: string) => {
      if (method === "POST") {
        const made = idle("t9", "zsh 2");
        list = [...list, made];
        return Promise.resolve(made) as never;
      }
      if (method === "GET" && path.endsWith("/terminals")) return Promise.resolve(list) as never;
      return Promise.resolve({ scripts: [] }) as never;
    });
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await screen.findByRole("tab", { name: "build" });
    expect(screen.getByText("Output of main").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "no");
    await user.click(screen.getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(screen.getByText("Output of t9").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "yes"));
    await user.click(screen.getByRole("tab", { name: "build" }));
    expect(screen.getByText("Output of t2").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "no");
  });

  it("asks before ending a shell that is alive, not only a script", async () => {
    serveLive([terminal({ script: null })]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close npm test" }));
    expect(await screen.findByRole("dialog", { name: "End this terminal?" })).toBeInTheDocument();
  });

  it("hides the main shell's tab, focuses its neighbour, and brings it back from a tab's menu", async () => {
    serveLive([idle("t2", "build")]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close Terminal" }));
    expect(screen.queryByRole("tab", { name: "Terminal" })).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("tab", { name: "build" })).toHaveFocus());
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: "build" }) });
    await user.click(screen.getByRole("menuitem", { name: "Show Terminal" }));
    expect(screen.getByRole("tab", { name: "Terminal" })).toBeInTheDocument();
  });

  it("puts focus on the terminal when the search closes, by Escape or by Close search", async () => {
    serveLive([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    const area = await screen.findByLabelText("xterm");
    area.focus();
    await user.keyboard("{Control>}f{/Control}");
    await screen.findByRole("textbox", { name: "Search the terminal" });
    await user.keyboard("{Escape}");
    expect(area).toHaveFocus();
    await user.keyboard("{Control>}f{/Control}");
    await user.click(await screen.findByRole("button", { name: "Close search" }));
    expect(area).toHaveFocus();
  });

  it("uses Cmd+F on a Mac and leaves Ctrl+F to the shell, and returns to the field when pressed again", async () => {
    const platform = vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
    serveLive([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    const area = await screen.findByLabelText("xterm");
    area.focus();
    await user.keyboard("{Control>}f{/Control}");
    expect(screen.queryByRole("textbox", { name: "Search the terminal" })).not.toBeInTheDocument();
    await user.keyboard("{Meta>}f{/Meta}");
    const field = await screen.findByRole("textbox", { name: "Search the terminal" });
    area.focus();
    await user.keyboard("{Meta>}f{/Meta}");
    expect(field).toHaveFocus();
    platform.mockRestore();
  });

  it("hears search results from a terminal that registers after the field opens", async () => {
    serveLive([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    (await screen.findByLabelText("xterm")).focus();
    await user.keyboard("{Control>}f{/Control}");
    await user.type(await screen.findByRole("textbox", { name: "Search the terminal" }), "x");
    const stop = registerTerminal(terminalKey("th", undefined), { clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: (listener) => { listener({ index: 0, count: 2 }); return () => undefined; } });
    expect(await screen.findByText("1 of 2")).toBeInTheDocument();
    stop();
  });
});

describe("closed terminal bar", () => {
  it("counts the processes still running and opens the drawer", async () => {
    serve([terminal({}), terminal({ id: "t3", group: "g3", alive: false })]);
    const onOpen = vi.fn();
    const user = userEvent.setup();
    render(<TerminalBar threadId="th" onOpen={onOpen} />);
    expect(await screen.findByRole("group", { name: "Terminal" })).toHaveTextContent("1 process running");
    await user.click(screen.getByRole("button", { name: "Open terminal" }));
    expect(onOpen).toHaveBeenCalled();
  });

  it("keeps asking slowly when nothing runs, and shows the bar once a script starts later", async () => {
    vi.useFakeTimers();
    try {
      let list: TerminalInfo[] = [];
      mock.mockImplementation(() => Promise.resolve(list) as never);
      render(<TerminalBar threadId="th" onOpen={() => undefined} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(20_000 + 40_000 + 60_000 + 60_000); });
      expect(screen.queryByRole("group", { name: "Terminal" })).not.toBeInTheDocument();
      list = [terminal({})];
      await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
      expect(screen.getByRole("group", { name: "Terminal" })).toHaveTextContent("1 process running");
    } finally {
      vi.useRealTimers();
    }
  });

  it("asks again at once when a terminal is revealed for the thread", async () => {
    vi.useFakeTimers();
    try {
      let list: TerminalInfo[] = [];
      mock.mockImplementation(() => Promise.resolve(list) as never);
      render(<TerminalBar threadId="th" onOpen={() => undefined} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
      list = [terminal({})];
      await act(async () => { useUi.getState().revealTerminal("th", "g2"); });
      await act(async () => { await vi.advanceTimersByTimeAsync(10); });
      expect(screen.getByRole("group", { name: "Terminal" })).toHaveTextContent("1 process running");
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not ask while the device is offline", async () => {
    serve([terminal({})]);
    useConnection.setState({ offline: true });
    render(<TerminalBar threadId="th" onOpen={() => undefined} />);
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mock).not.toHaveBeenCalled();
    useConnection.setState({ offline: false });
  });

  it("is not there when nothing runs", async () => {
    serve([terminal({ alive: false })]);
    render(<TerminalBar threadId="th" onOpen={() => undefined} />);
    await waitFor(() => expect(mock).toHaveBeenCalled());
    expect(screen.queryByRole("group", { name: "Terminal" })).not.toBeInTheDocument();
  });
});
