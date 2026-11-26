import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire } from "../../../wire.test-support";
import type { TerminalInfo } from "../../../api/types";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { TerminalBar, TerminalDrawerBody } from "./TerminalDrawerBody";
import { runHandlers } from "../../../product/shell/shortcuts";
import { registerTerminal, terminalKey } from "./terminalControls";
import { useAttention } from "./attention/store";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId, autoFocus }: { terminalId?: string; autoFocus?: boolean }) => <div data-autofocus={autoFocus ? "yes" : "no"}>Output of {terminalId ?? "main"}<textarea className="xterm-helper-textarea" aria-label="xterm" /></div> }));

let mock: MockApi;
const deletes = () => wire().filter((call) => call.method === "DELETE").map((call) => call.path);
const terminalReads = () => wire().filter((call) => call.method === "GET" && call.path === "/api/threads/th/terminals");
const opened = () => wire().filter((call) => call.method === "POST" && call.path === "/api/threads/th/terminals").map((call) => call.body);

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

/** The terminals the service lists for the thread, and the project's one script. */
function serve(terminals: TerminalInfo[]) {
  mock.terminals.seed("th", terminals);
  mock.terminals.setScripts("th", [{ name: "test", icon: "test", command: "npm test" }]);
}

beforeEach(() => {
  mock = installMockFetch();
  useAttention.getState().reset();
  useComposerChips.setState({ chips: {} });
  useUi.setState({ terminalOpen: true, toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const show = async (terminals: TerminalInfo[]) => {
  serve(terminals);
  const user = userEvent.setup();
  render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
  await user.click(await screen.findByRole("tab", { name: /npm test/ }));
  return user;
};

describe("terminal drawer", () => {
  it("puts an amber dot on the tab of a terminal that waits for the person, and takes it away when it is answered or dismissed", async () => {
    await show([terminal({ id: "t2", group: "g2" }), terminal({ id: "t3", group: "g3", title: "build" })]);
    const at = (seq: number, value: unknown) => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: "th", event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });
    const waiting = { v: 1, op: "waiting", id: "a1", terminal_id: "t3", terminal_title: "build", engine: "claude", kind: "question", since: "2026-11-22T14:12:00Z", excerpt: ["Continue?"] };
    expect(document.querySelector(".ptab .ptab-attn")).toBeNull();
    act(() => { useAttention.getState().receive(at(1, waiting)); });
    expect(screen.getByRole("tab", { name: "build, waiting for you" }).querySelector(".ptab-attn")).not.toBeNull();
    expect(screen.getByRole("tab", { name: /npm test/ }).querySelector(".ptab-attn")).toBeNull();
    act(() => { useAttention.getState().dismiss("a1"); });
    expect(document.querySelector(".ptab-attn")).toBeNull();
    act(() => { useAttention.getState().reset(); useAttention.getState().receive(at(2, { ...waiting, id: "a2" })); });
    expect(document.querySelector(".ptab-attn")).not.toBeNull();
    act(() => { useAttention.getState().receive(at(3, { v: 1, op: "clear", id: "a2", reason: "input" })); });
    expect(document.querySelector(".ptab-attn")).toBeNull();
  });

  it("puts the frame's silence strip under the tabs for the waiting console on screen, and takes it away when the event clears", async () => {
    await show([terminal({ id: "t2", group: "g2" })]);
    const at = (seq: number, value: unknown) => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: "th", event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });
    const waiting = { v: 1, op: "waiting", id: "a1", terminal_id: "t2", terminal_title: "npm test", engine: "claude", kind: "yes_no", since: new Date(Date.now() - 38_000).toISOString(), excerpt: ["Overwrite auth/redirect.ts with the new version? (y/N)"] };
    expect(document.querySelector(".term-wait")).toBeNull();
    act(() => { useAttention.getState().receive(at(1, waiting)); });
    const strip = document.querySelector(".term-wait");
    expect(strip?.textContent).toBe("Waiting for you · Claude Code has been silent for 38 s");
    expect(strip?.previousElementSibling?.classList.contains("terminal-tabs")).toBe(true);
    for (const absent of ["Answer here", "Yes", "No"]) expect(strip?.textContent).not.toContain(absent);
    act(() => { useAttention.getState().receive(at(2, { v: 1, op: "clear", id: "a1", reason: "input" })); });
    expect(document.querySelector(".term-wait")).toBeNull();
  });

  it("marks a tab by how its process is doing and closes it from its own button", async () => {
    const user = await show([terminal({}), terminal({ id: "t3", group: "g3", title: "build", alive: false, exit_code: 0 }), terminal({ id: "t4", group: "g4", title: "lint", alive: false, exit_code: 2 })]);
    expect(screen.getAllByLabelText("Working")).not.toHaveLength(0);
    expect(screen.getByLabelText("Finished")).toBeInTheDocument();
    expect(screen.getByLabelText("Failed")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close build" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t3"]));
  });

  it("shows how long a running process has gone, and clears or interrupts it", async () => {
    const clear = vi.fn();
    const interrupt = vi.fn();
    const stop = registerTerminal(terminalKey("th", "t2"), { clear, write: vi.fn(), interrupt, output: () => "", find: vi.fn(), onResults: () => () => undefined });
    const user = await show([terminal({})]);
    expect(screen.getByText(/^Running · 1m 0\ds$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(clear).toHaveBeenCalled();
    expect(interrupt).toHaveBeenCalled();
    stop();
  });

  it("says a command failed with its code, sends the output to the agent and runs it again", async () => {
    const stop = registerTerminal(terminalKey("th", "t2"), { clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "1 failed | 47 passed", find: vi.fn(), onResults: () => () => undefined });
    const user = await show([terminal({ alive: false, exit_code: 1 })]);
    expect(screen.getByText("Exited with code 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Send to the agent" }));
    expect(useComposerChips.getState().chips["th"]).toEqual([expect.objectContaining({ kind: "terminal", output: "1 failed | 47 passed", exitCode: 1 })]);
    await user.click(screen.getByRole("button", { name: "Run again" }));
    await waitFor(() => expect(wire().filter((call) => call.method === "POST" && call.path === "/api/threads/th/scripts")).toEqual([{ method: "POST", path: "/api/threads/th/scripts", body: { name: "test" } }]));
    stop();
  });

  it("keeps the output of a process that ended and offers Close tab", async () => {
    const user = await show([terminal({ alive: false, exit_code: 0 })]);
    expect(screen.getByText("Ended")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close tab" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t2"]));
  });

  it("offers Clear, Copy and Watch from a tab's right-click menu, and End process only while it runs, after the split choices", async () => {
    const user = await show([terminal({})]);
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: /npm test/ }) });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Split right", "Split down", "Clear output", "Copy all output", "Clear all blocks…", "Watch only", "End process"]);
  });

  it("disables Clear and Copy for a tab that is not showing, and never says Output copied when nothing was copied", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const user = await show([terminal({}), terminal({ id: "t3", group: "g3", title: "build", alive: false, exit_code: 0 })]);
    const stop = registerTerminal(terminalKey("th", "t2"), { clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
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
    expect(deletes()).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "End" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t2"]));
    await user.click(screen.getByRole("button", { name: "Close build" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t2", "/api/threads/th/terminals/t3"]));
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
    await waitFor(() => expect(opened()).toEqual([{}]));
  });

  it("says No terminals open once every tab is closed, and offers New terminal", async () => {
    serve([]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Close Terminal" }));
    expect(screen.getByText("No terminals open.")).toBeInTheDocument();
    await user.click(within(screen.getByText("No terminals open.").parentElement!).getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(opened()).toEqual([{}]));
  });

  it("searches the output: Ctrl+F opens the field, Enter moves to the next match, and the count says where you are", async () => {
    const find = vi.fn();
    const stop = registerTerminal(terminalKey("th", undefined), { clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find, onResults: (listener) => { listener({ index: 1, count: 3 }); return () => undefined; } });
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
  /** The service forgets a terminal once it has been deleted, so the next list leaves it out. */
  function serveLive(initial: TerminalInfo[]) {
    serve(initial);
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
    expect(deletes()).toEqual([]);
  });

  it("asks a terminal opened here to take focus when it loads, and not one that was already there", async () => {
    serve([idle("t2", "build")]);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await screen.findByRole("tab", { name: "build" });
    expect(screen.getByText("Output of main").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "no");
    await user.click(screen.getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(screen.getByText("Output of t-new1").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "yes"));
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
    const stop = registerTerminal(terminalKey("th", undefined), { clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: (listener) => { listener({ index: 0, count: 2 }); return () => undefined; } });
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
      serve([]);
      render(<TerminalBar threadId="th" onOpen={() => undefined} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(20_000 + 40_000 + 60_000 + 60_000); });
      expect(screen.queryByRole("group", { name: "Terminal" })).not.toBeInTheDocument();
      serve([terminal({})]);
      await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
      expect(screen.getByRole("group", { name: "Terminal" })).toHaveTextContent("1 process running");
    } finally {
      vi.useRealTimers();
    }
  });

  it("asks again at once when a terminal is revealed for the thread", async () => {
    vi.useFakeTimers();
    try {
      serve([]);
      render(<TerminalBar threadId="th" onOpen={() => undefined} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
      serve([terminal({})]);
      await act(async () => { useUi.getState().revealTerminal("th", "g2"); });
      await act(async () => { await vi.advanceTimersByTimeAsync(10); });
      expect(screen.getByRole("group", { name: "Terminal" })).toHaveTextContent("1 process running");
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not ask while the device is offline", async () => {
    vi.useFakeTimers();
    try {
      serve([terminal({})]);
      useConnection.setState({ offline: true });
      render(<TerminalBar threadId="th" onOpen={() => undefined} />);
      // Long enough for the bar's slowest poll to have come due.
      await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
      expect(wire()).toEqual([]);
    } finally {
      vi.useRealTimers();
      useConnection.setState({ offline: false });
    }
  });

  it("is not there when nothing runs", async () => {
    serve([terminal({ alive: false })]);
    render(<TerminalBar threadId="th" onOpen={() => undefined} />);
    await waitFor(() => expect(terminalReads()).not.toHaveLength(0));
    expect(screen.queryByRole("group", { name: "Terminal" })).not.toBeInTheDocument();
  });
});

describe("terminal split and vertical split", () => {
  /** The service adds a pane to the group of `split_of`, as the real one does. */
  const serveSplits = (initial: TerminalInfo[]) => serve(initial);
  const posts = opened;
  const shell = (over: Partial<TerminalInfo> = {}) => terminal({ id: "t2", group: "g2", title: "dev", script: null, ...over });
  const mountSplits = async () => {
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /^dev/ }));
    return user;
  };

  it("splits the shown terminal to the right from the header, each pane with its own terminal", async () => {
    serveSplits([shell()]);
    const user = await mountSplits();
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    await screen.findByText("Output of t-new1");
    expect(posts()).toEqual([{ split_of: "t2", direction: "horizontal" }]);
    expect(screen.getByText("Output of t2")).toBeInTheDocument();
    expect(document.querySelector(".terminal-splits")).toHaveAttribute("data-direction", "horizontal");
    expect(screen.getByText("Output of t-new1").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "yes");
    expect(screen.getByText("Output of t2").closest("[data-autofocus]")).toHaveAttribute("data-autofocus", "no");
  });

  it("splits right or down from the tab menu", async () => {
    serveSplits([shell(), shell({ id: "t3", group: "g3", title: "lint" })]);
    const user = await mountSplits();
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: "dev" }) });
    await user.click(screen.getByRole("menuitem", { name: "Split down" }));
    await screen.findByText("Output of t-new1");
    expect(posts()).toEqual([{ split_of: "t2", direction: "vertical" }]);
    expect(document.querySelector(".terminal-splits")).toHaveAttribute("data-direction", "vertical");
  });

  it("splits from the shortcuts", async () => {
    serveSplits([shell()]);
    await mountSplits();
    act(() => void runHandlers("terminal.split"));
    await screen.findByText("Output of t-new1");
    expect(posts()).toEqual([{ split_of: "t2", direction: "horizontal" }]);
  });

  it("splits vertically from its shortcut", async () => {
    serveSplits([shell()]);
    await mountSplits();
    act(() => void runHandlers("terminal.splitVertical"));
    await screen.findByText("Output of t-new1");
    expect(posts()).toEqual([{ split_of: "t2", direction: "vertical" }]);
  });

  it("opens one terminal for a press that arrives while the first is still opening, and writes nothing by itself", async () => {
    serveSplits([shell()]);
    const user = await mountSplits();
    const slow = holdFetch("POST", "/api/threads/th/terminals");
    expect(posts()).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    act(() => void runHandlers("terminal.split"));
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    expect(posts()).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Split pane" })).toBeDisabled();
    await act(async () => slow.release());
    await screen.findByText("Output of t-new1");
    expect(screen.getByRole("button", { name: "Split pane" })).toBeEnabled();
  });

  it("keeps the direction of a group that is split already and stops at four panes", async () => {
    serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal" })]);
    const user = await mountSplits();
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: "dev (2)" }) });
    expect(screen.getByRole("menuitem", { name: "Split right" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Split down" })).toBeDisabled();
    await user.keyboard("{Escape}");
    act(() => void runHandlers("terminal.splitVertical"));
    expect(posts()).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    await screen.findByText("Output of t-new1");
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    await screen.findByText("Output of t-new2");
    expect(posts()).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Split pane" })).toBeDisabled();
    act(() => void runHandlers("terminal.split"));
    expect(posts()).toHaveLength(2);
  });

  it("closes one pane's terminal from its own button and leaves the others", async () => {
    serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal", alive: false, exit_code: 0 })]);
    const user = await mountSplits();
    await user.click(screen.getByRole("button", { name: "Close pane two" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t3"]));
    await waitFor(() => expect(screen.queryByText("Output of t3")).not.toBeInTheDocument());
    expect(screen.getByText("Output of t2")).toBeInTheDocument();
  });

  it("asks before it ends a pane whose shell is alive, as it does for a tab", async () => {
    serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal" })]);
    const user = await mountSplits();
    await user.click(screen.getByRole("button", { name: "Close pane two" }));
    expect(deletes()).toEqual([]);
    await user.click(within(await screen.findByRole("dialog", { name: "End this terminal?" })).getByRole("button", { name: "End" }));
    await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t3"]));
  });

  describe("a tab's own X ends every pane of the tab (frames x-term-tab-close, x-term-end-confirm)", () => {
    it("asks once for the whole tab when any shell runs, names the count and the running one, and ends them all with one click each", async () => {
      serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal", alive: false, exit_code: 0 })]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close dev" }));
      const dialog = await screen.findByRole("dialog", { name: "End this tab?" });
      expect(dialog).toHaveTextContent("This tab has 2 terminals and dev is still running. Ending the tab ends both of them and stops dev.");
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
      expect(deletes()).toEqual([]);
      const end = within(dialog).getByRole("button", { name: "End all" });
      await user.dblClick(end);
      await waitFor(() => expect(deletes().sort()).toEqual(["/api/threads/th/terminals/t2", "/api/threads/th/terminals/t3"]));
      await waitFor(() => expect(screen.queryByRole("tab", { name: /^dev/ })).not.toBeInTheDocument());
    });

    it("asks when only a later pane is still running", async () => {
      serveSplits([shell({ alive: false, exit_code: 0 }), shell({ id: "t3", title: "two", direction: "horizontal" })]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close dev" }));
      expect(await screen.findByRole("dialog", { name: "End this tab?" })).toHaveTextContent("two is still running");
      expect(deletes()).toEqual([]);
    });

    it("closes a tab whose shells have all finished without asking, every pane of it", async () => {
      serveSplits([shell({ alive: false, exit_code: 0 }), shell({ id: "t3", title: "two", direction: "horizontal", alive: false, exit_code: 0 })]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close dev" }));
      await waitFor(() => expect(deletes().sort()).toEqual(["/api/threads/th/terminals/t2", "/api/threads/th/terminals/t3"]));
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("keeps the terminal wording for a tab of one pane", async () => {
      serveSplits([shell()]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close dev" }));
      expect(await screen.findByRole("dialog", { name: "End this terminal?" })).toHaveTextContent("dev is still running. Ending the terminal stops it.");
    });

    it("Cancel ends nothing, and End process in the menu and the close shortcut act on the whole tab", async () => {
      serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal" })]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close dev" }));
      await user.click(within(await screen.findByRole("dialog", { name: "End this tab?" })).getByRole("button", { name: "Cancel" }));
      expect(deletes()).toEqual([]);
      await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: "dev (2)" }) });
      await user.click(screen.getByRole("menuitem", { name: "End process" }));
      expect(await screen.findByRole("dialog", { name: "End this tab?" })).toBeVisible();
      await user.click(screen.getByRole("button", { name: "Cancel" }));
      act(() => void runHandlers("terminal.close"));
      expect(await screen.findByRole("dialog", { name: "End this tab?" })).toBeVisible();
      expect(deletes()).toEqual([]);
    });

    it("a pane's own X still ends only that pane", async () => {
      serveSplits([shell(), shell({ id: "t3", title: "two", direction: "horizontal", alive: false, exit_code: 0 })]);
      const user = await mountSplits();
      await user.click(screen.getByRole("button", { name: "Close pane two" }));
      await waitFor(() => expect(deletes()).toEqual(["/api/threads/th/terminals/t3"]));
    });
  });

  it("offers no pane close for the main shell, and no split before the service lists it", async () => {
    serveSplits([]);
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await screen.findByRole("tab", { name: "Terminal" });
    expect(screen.getByRole("button", { name: "Split pane" })).toBeDisabled();
    act(() => void runHandlers("terminal.split"));
    expect(posts()).toEqual([]);
  });

  it("says when the split could not be made and adds no pane", async () => {
    serveSplits([shell()]);
    const user = await mountSplits();
    mock.fail("/api/threads/th/terminals", 1, 403, "folder_untrusted", "Trust this folder before running an engine in it.");
    await user.click(screen.getByRole("button", { name: "Split pane" }));
    await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text).join()).toContain("Couldn't do that. Trust this folder before running an engine in it."));
    expect(screen.queryByText("Output of t-new1")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Split pane" })).toBeEnabled();
  });
});
