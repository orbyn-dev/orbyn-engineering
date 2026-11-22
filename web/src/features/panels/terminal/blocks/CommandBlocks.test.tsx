import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../../api/client";
import { useUi } from "../../../../product/state/ui";
import { blocksApi, setBlocksApi, type BlockList, type BlockOutputPage, type CommandBlock, type CommandBlocksApi } from "./blocksContract";
import { realBlocksApi } from "./blocksClient";
import { CommandBlocks } from "./CommandBlocks";

const T0 = Date.UTC(2026, 10, 22, 14, 0, 0);
const at = (seconds: number) => new Date(T0 + seconds * 1000).toISOString();

const block = (over: Partial<CommandBlock> = {}): CommandBlock => ({
  id: "b1", command: "git status", started_at: at(0), ended_at: at(0.1), exit_code: 0, cwd: "/work", output_bytes: 40,
  truncated: false, dropped_lines: 0, fullscreen: false, ...over,
});
const list = (blocks: CommandBlock[], over: Partial<BlockList> = {}): BlockList => ({
  blocks, next_before: null, limits: { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 }, ...over,
});
const page = (text: string, over: Partial<BlockOutputPage> = {}): BlockOutputPage => ({ text, next: null, total_lines: text === "" ? 0 : text.split("\n").length, dropped_lines: 0, ...over });

function install(blocks: CommandBlock[], outputs: Record<string, BlockOutputPage | BlockOutputPage[]> = {}, over: Partial<BlockList> = {}) {
  const api = {
    list: vi.fn((_t, _p, _s) => Promise.resolve(list(blocks, over))),
    output: vi.fn((_t, id: string, p: { after?: string }) => {
      const found = Object.hasOwn(outputs, id) ? outputs[id] : undefined;
      const pages = found === undefined ? [page("")] : Array.isArray(found) ? found : [found];
      const index = p.after === undefined ? 0 : Number(p.after);
      return Promise.resolve(pages[index] ?? page(""));
    }),
  } satisfies CommandBlocksApi;
  setBlocksApi(api);
  return api;
}

const props = { threadId: "th", terminalId: "main" };
const mount = (extra: Partial<Parameters<typeof CommandBlocks>[0]> = {}) => render(<CommandBlocks {...props} {...extra} />);
const group = (command: string) => screen.getByRole("group", { name: `Command ${command}` });

beforeEach(() => useUi.setState({ toasts: [] }));
afterEach(() => {
  setBlocksApi(null);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("the production client", () => {
  const settle = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

  it("draws nothing and says nothing when its first list is empty", async () => {
    const limits = { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 };
    const fetching = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({ blocks: [], next_before: null, limits }), { status: 200 }));
    vi.stubGlobal("fetch", fetching);
    setBlocksApi(null);
    expect(blocksApi()).toBe(realBlocksApi);
    const { container } = mount();
    await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
    expect(String(fetching.mock.calls[0]?.[0])).toBe("/api/threads/th/terminals/main/blocks");
    await settle();
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText("Commands you run here appear as blocks")).not.toBeInTheDocument();
    expect(useUi.getState().toasts).toEqual([]);
  });

  it("draws nothing and says nothing when its list fails", async () => {
    const fetching = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({ code: "internal", message: "No." }), { status: 500 }));
    vi.stubGlobal("fetch", fetching);
    setBlocksApi(null);
    const { container } = mount();
    await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
    await settle();
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText("Couldn't read this terminal's blocks.")).not.toBeInTheDocument();
    expect(useUi.getState().toasts).toEqual([]);
  });
});

describe("the blocks", () => {
  it("draws a header per command with its status, exit code and how long it took", async () => {
    install([block(), block({ id: "b2", command: "npm test", exit_code: 1, ended_at: at(4.2) })], { b2: page("FAIL e2e/login.spec.ts") });
    mount();
    const ok = await screen.findByRole("group", { name: "Command git status" });
    expect(within(ok).getByText("exit 0 · 0.1 s")).toBeInTheDocument();
    expect(ok).not.toHaveAttribute("data-failed");
    const failed = group("npm test");
    expect(within(failed).getByText("exit 1 · 4.2 s")).toBeInTheDocument();
    expect(failed).toHaveAttribute("data-failed", "true");
    expect(await within(failed).findByText("FAIL e2e/login.spec.ts")).toBeInTheDocument();
  });

  it("says no exit code when the shell sent none, and shows a command-less block by its place", async () => {
    install([block({ exit_code: null, ended_at: at(3.1) }), block({ id: "b2", command: null })]);
    mount();
    expect(await screen.findByText("no exit code · 3.1 s")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Command not reported" })).toBeInTheDocument();
  });

  it("shows a running block with a spinner and a clock where the exit code will go, and an output that keeps arriving", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(T0 + 42_000);
    const running = block({ id: "r", command: "npm run dev", ended_at: null, exit_code: null });
    const api = install([running], { r: page("VITE ready", { total_lines: null }) });
    mount();
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    const card = group("npm run dev");
    expect(within(card).getByLabelText("Running")).toBeInTheDocument();
    expect(within(card).getByText("0:42")).toBeInTheDocument();
    expect(within(card).getByText("VITE ready")).toBeInTheDocument();
    const reads = api.output.mock.calls.length;
    await act(async () => { await vi.advanceTimersByTimeAsync(2_100); });
    expect(api.output.mock.calls.length).toBeGreaterThan(reads);
    expect(within(card).getByText("0:44")).toBeInTheDocument();
  });

  it("stops asking once nothing runs", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    const api = install([block()]);
    mount();
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    const calls = api.list.mock.calls.length;
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(api.list.mock.calls.length).toBe(calls);
  });

  it("says what blocks are when there are none", async () => {
    install([]);
    mount();
    expect(await screen.findByText("Commands you run here appear as blocks")).toBeInTheDocument();
  });

  it("says it could not read the blocks and tries again on the person's click only", async () => {
    const api = install([block()]);
    api.list.mockRejectedValueOnce(new ApiFailure("boom", "No.", 500));
    const user = userEvent.setup();
    mount();
    expect(await screen.findByText("Couldn't read this terminal's blocks.")).toBeInTheDocument();
    expect(api.list).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("group", { name: "Command git status" })).toBeInTheDocument();
    expect(api.list).toHaveBeenCalledTimes(2);
  });

  it("asks for the page before with the cursor, once per click", async () => {
    const api = install([block()], {}, { next_before: "c1" });
    const user = userEvent.setup();
    mount();
    await user.click(await screen.findByRole("button", { name: "Show earlier commands" }));
    expect(api.list).toHaveBeenLastCalledWith({ threadId: "th", terminalId: "main" }, { before: "c1" }, expect.anything());
  });

  it("keeps a block whose id is __proto__ apart from the others", async () => {
    install([block({ id: "__proto__", command: "one" }), block({ id: "constructor", command: "two" })], Object.fromEntries([["__proto__", page("A")], ["constructor", page("B")]]));
    const user = userEvent.setup();
    mount();
    await screen.findByRole("group", { name: "Command one" });
    await user.click(within(group("one")).getByRole("button", { name: "Collapse" }));
    expect(within(group("two")).getByRole("button", { name: "Collapse" })).toBeInTheDocument();
    expect(within(group("one")).getByRole("button", { name: "Expand" })).toBeInTheDocument();
  });
});

describe("terminal text is text", () => {
  it("draws markup and escape sequences as plain characters, never as elements", async () => {
    install([block({ command: "<b>cmd</b>" })], { b1: page("<img src=x onerror=alert(1)>\n\u001b[31mred\u001b[0m <a href=\"javascript:1\">x</a>") });
    const { container } = mount();
    expect(await screen.findByText(/onerror=alert/)).toBeInTheDocument();
    expect(container.querySelector("img, b, a")).toBeNull();
    expect(container.textContent).not.toContain("\u001b");
    expect(screen.getByText(/red <a href/)).toBeInTheDocument();
    expect(screen.getByText("<b>cmd</b>")).toBeInTheDocument();
  });
});

describe("collapse, select and the keyboard", () => {
  it("collapses to one line and expands again; the output goes and comes back", async () => {
    install([block({ command: "npm test" })], { b1: page("all green") });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    expect(await within(card).findByText("all green")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Collapse" }));
    expect(within(card).queryByText("all green")).not.toBeInTheDocument();
    expect(within(card).getByText("exit 0 · 0.1 s")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Expand" }));
    expect(await within(card).findByText("all green")).toBeInTheDocument();
  });

  it("selects a block by a click, one at a time, and a click on a button does not also select or collapse", async () => {
    install([block(), block({ id: "b2", command: "npm test" })], { b1: page("x"), b2: page("y") });
    const user = userEvent.setup();
    mount();
    await screen.findByText("y");
    expect(group("git status")).not.toHaveAttribute("data-selected");
    await user.click(within(group("git status")).getByText("exit 0 · 0.1 s"));
    expect(group("git status")).toHaveAttribute("data-selected", "true");
    await user.click(within(group("npm test")).getByText("exit 0 · 0.1 s"));
    expect(group("git status")).not.toHaveAttribute("data-selected");
    expect(group("npm test")).toHaveAttribute("data-selected", "true");
    // A button inside a block does its own thing only.
    await user.click(within(group("git status")).getByRole("button", { name: "Collapse" }));
    expect(group("git status")).not.toHaveAttribute("data-selected");
    expect(group("npm test")).toHaveAttribute("data-selected", "true");
  });

  it("moves between blocks with Tab and toggles collapse with Enter", async () => {
    install([block({ command: "one" }), block({ id: "b2", command: "two" })], { b1: page("x"), b2: page("y") });
    const user = userEvent.setup();
    mount();
    await screen.findByText("y");
    group("one").focus();
    await user.keyboard("{Enter}");
    expect(within(group("one")).queryByText("x")).not.toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(await within(group("one")).findByText("x")).toBeInTheDocument();
    // Enter on a button inside the block presses the button, it does not also collapse the block.
    within(group("one")).getByRole("button", { name: "Copy output" }).focus();
    Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn(() => Promise.resolve()) }, configurable: true });
    await user.keyboard("{Enter}");
    expect(within(group("one")).getByText("x")).toBeInTheDocument();
  });

  it("collapses every block from More, and a block that shows up afterwards starts open", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    const running = block({ id: "b2", command: "two", ended_at: null, exit_code: null });
    const api = install([block({ command: "one" }), running], { b1: page("x"), b2: page("y", { total_lines: null }) });
    mount();
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(screen.getByText("y")).toBeInTheDocument();
    fireEvent.click(within(group("one")).getByRole("button", { name: "More" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Collapse all" }));
    expect(screen.queryByText("x")).not.toBeInTheDocument();
    expect(screen.queryByText("y")).not.toBeInTheDocument();
    api.list.mockResolvedValue(list([block({ command: "one" }), running, block({ id: "b3", command: "three" })]));
    api.output.mockResolvedValue(page("z"));
    await act(async () => { await vi.advanceTimersByTimeAsync(2_100); });
    expect(screen.getByText("z")).toBeInTheDocument();
    expect(within(group("one")).getByRole("button", { name: "Expand" })).toBeInTheDocument();
  });
});

describe("copy", () => {
  let writeText: ReturnType<typeof vi.fn>;
  beforeEach(() => {
    writeText = vi.fn(() => Promise.resolve());
  });
  /** user-event installs its own clipboard, so ours goes in after it. */
  const person = () => {
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    return user;
  };
  const toasts = () => useUi.getState().toasts.map((toast) => toast.text);

  it("copies the whole output of a block, every page, with escapes gone, once per click", async () => {
    const pages = [page("a\n\u001b[31mb\u001b[0m\n", { next: "1" }), page("c\n")];
    const api = install([block({ command: "npm test" })], { b1: pages });
    const user = person();
    mount();
    await within(await screen.findByRole("group", { name: "Command npm test" })).findByText(/^a/);
    api.output.mockClear();
    await user.click(screen.getByRole("button", { name: "Copy output" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText).toHaveBeenCalledWith("a\nb\nc\n");
    expect(toasts()).toEqual(["Output copied"]);
  });

  it("copies the command and says so", async () => {
    install([block({ command: "npm test" })]);
    const user = person();
    mount();
    await screen.findByRole("group", { name: "Command npm test" });
    await user.click(screen.getByRole("button", { name: "More" }));
    await user.click(screen.getByRole("menuitem", { name: "Copy command" }));
    expect(writeText).toHaveBeenCalledWith("npm test");
    expect(toasts()).toEqual(["Command copied"]);
  });

  it("copies both as the command line, then the output", async () => {
    install([block({ command: "npm test" })], { b1: page("ok\n") });
    const user = person();
    mount();
    await screen.findByText("ok");
    await user.click(screen.getByRole("button", { name: "More" }));
    await user.click(screen.getByRole("menuitem", { name: "Copy both" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("$ npm test\nok\n"));
  });

  it("never says copied when the clipboard refused", async () => {
    writeText.mockRejectedValue(new Error("no"));
    install([block()], { b1: page("ok") });
    const user = person();
    mount();
    await screen.findByText("ok");
    await user.click(screen.getByRole("button", { name: "Copy output" }));
    await waitFor(() => expect(toasts()).toEqual(["Couldn't copy the output"]));
  });

  it("offers no Copy output on a full-screen program's block, and says its output is not kept", async () => {
    install([block({ command: "vim notes.md", fullscreen: true, output_bytes: 0 })]);
    mount();
    const card = await screen.findByRole("group", { name: "Command vim notes.md" });
    expect(within(card).getByText("Full-screen program, output not kept")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Copy output" })).not.toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Run again" })).toBeInTheDocument();
  });
});

describe("run again", () => {
  it("writes the command into the terminal on a click, with no newline, and runs nothing itself", async () => {
    install([block({ command: "npm test -- --run" })]);
    const write = vi.fn();
    const user = userEvent.setup();
    mount({ write });
    await user.click(await screen.findByRole("button", { name: "Run again" }));
    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith("npm test -- --run");
  });

  it("writes nothing until the click, however often the blocks reload", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    const write = vi.fn();
    install([block({ command: "dev", ended_at: null, exit_code: null })]);
    mount({ write });
    await act(async () => { await vi.advanceTimersByTimeAsync(7_000); });
    expect(write).not.toHaveBeenCalled();
  });

  it("is off when the terminal cannot take a write, and for a command that is not one plain line", async () => {
    install([block({ command: "ls\nrm -rf x" }), block({ id: "b2", command: "ls", output_bytes: 0 })]);
    mount();
    await screen.findByRole("group", { name: "Command ls" });
    expect(within(group("ls")).getByRole("button", { name: "Run again" })).toBeDisabled();
    // The first block's command is cleaned for the header; its Run again is off because the line was not one line.
    const first = screen.getAllByRole("button", { name: "Run again" })[0];
    expect(first).toBeDisabled();
  });

  it("runs nothing again while a block runs, and offers Stop, which writes Ctrl-C once per click", async () => {
    install([block({ id: "r", command: "npm run dev", ended_at: null, exit_code: null })]);
    const interrupt = vi.fn();
    const user = userEvent.setup();
    mount({ write: vi.fn(), interrupt });
    await user.click(await screen.findByRole("button", { name: "Stop" }));
    expect(interrupt).toHaveBeenCalledTimes(1);
  });
});

describe("long output", () => {
  const long = Array.from({ length: 20 }, (_, index) => `line ${index + 1}`).join("\n");

  it("shows the last lines, says how many there are, and Show all opens it in place", async () => {
    install([block({ command: "npm run build" })], { b1: page(long) });
    const user = userEvent.setup();
    mount();
    expect(await screen.findByText("Showing the last 6 of 20 lines")).toBeInTheDocument();
    expect(screen.getByText(/line 20/)).toBeInTheDocument();
    expect(document.querySelector(".cb-out")?.textContent).toBe(long.split("\n").slice(-6).join("\n"));
    await user.click(screen.getByRole("button", { name: "Show all" }));
    expect(document.querySelector(".cb-out")?.textContent).toBe(long);
    expect(screen.queryByText(/Showing the last/)).not.toBeInTheDocument();
  });

  it("says how many earlier lines the service dropped", async () => {
    install([block({ truncated: true, dropped_lines: 300 })], { b1: page(long, { total_lines: 20, dropped_lines: 300 }) });
    mount();
    expect(await screen.findByText("Showing the last 6 of 20 lines · 300 earlier lines were dropped")).toBeInTheDocument();
  });
});

describe("filter output", () => {
  const seven = "FAIL here\nline two\nline three\nline four\nline five\nline six\nline seven";

  it("keeps the lines with the typed text, ignores case, says 1 of 7 lines and draws no line numbers", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    expect(await within(card).findByText(/line seven/)).toBeInTheDocument();
    // The matching line is the first of seven, hidden by the default six-line tail.
    expect(within(card).queryByText(/FAIL here/)).not.toBeInTheDocument();

    const control = within(card).getByRole("button", { name: "Filter output" });
    await user.click(control);
    const field = within(card).getByRole("textbox", { name: "Filter output" });
    expect(field).toHaveFocus();
    await user.type(field, "fail");

    expect(within(card).getByText("1 of 7 lines")).toBeInTheDocument();
    expect(card.querySelector(".cb-out")?.textContent).toBe("FAIL here");
    expect(within(card).queryByText("line seven")).not.toBeInTheDocument();
    expect(control).toHaveAttribute("aria-pressed", "true");
  });

  it("clears with Show all and returns focus to the control", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    const control = within(card).getByRole("button", { name: "Filter output" });
    await user.click(control);
    await user.type(within(card).getByRole("textbox", { name: "Filter output" }), "fail");
    await user.click(within(card).getByRole("button", { name: "Show all" }));

    expect(within(card).queryByRole("textbox", { name: "Filter output" })).not.toBeInTheDocument();
    expect(control).toHaveFocus();
    expect(card.querySelector(".cb-out")?.textContent).toBe(seven.split("\n").slice(-6).join("\n"));
  });

  it("clears with Esc and returns focus to the control", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    const control = within(card).getByRole("button", { name: "Filter output" });
    await user.click(control);
    const field = within(card).getByRole("textbox", { name: "Filter output" });
    await user.type(field, "fail");
    await user.keyboard("{Escape}");

    expect(within(card).queryByRole("textbox", { name: "Filter output" })).not.toBeInTheDocument();
    expect(control).toHaveFocus();
    expect(card.querySelector(".cb-out")?.textContent).toBe(seven.split("\n").slice(-6).join("\n"));
  });

  it("is dropped when the block is collapsed and is not restored on expand", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    await user.click(within(card).getByRole("button", { name: "Filter output" }));
    await user.type(within(card).getByRole("textbox", { name: "Filter output" }), "fail");
    await user.click(within(card).getByRole("button", { name: "Collapse" }));
    expect(within(card).queryByRole("textbox", { name: "Filter output" })).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Expand" }));

    expect(within(card).queryByRole("textbox", { name: "Filter output" })).not.toBeInTheDocument();
    expect(card.querySelector(".cb-out")?.textContent).toBe(seven.split("\n").slice(-6).join("\n"));
  });

  it("applies to its own block only", async () => {
    install([block({ command: "one" }), block({ id: "b2", command: "two" })], { b1: page("alpha fail\nbeta"), b2: page("gamma fail\ndelta") });
    const user = userEvent.setup();
    mount();
    const first = await screen.findByRole("group", { name: "Command one" });
    const second = group("two");
    await user.click(within(first).getByRole("button", { name: "Filter output" }));
    await user.type(within(first).getByRole("textbox", { name: "Filter output" }), "fail");

    expect(within(first).getByText("1 of 2 lines")).toBeInTheDocument();
    expect(within(second).queryByRole("textbox", { name: "Filter output" })).not.toBeInTheDocument();
    expect(within(second).getByText(/gamma fail/)).toBeInTheDocument();
  });

  it("filters only the live text a running block holds", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(T0 + 42_000);
    const running = block({ id: "r", command: "npm run dev", ended_at: null, exit_code: null });
    const api = install([running], { r: page("first chunk", { total_lines: null }) });
    mount();
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    const card = group("npm run dev");
    fireEvent.click(within(card).getByRole("button", { name: "Filter output" }));
    fireEvent.change(within(card).getByRole("textbox", { name: "Filter output" }), { target: { value: "second" } });
    expect(within(card).getByText("0 of 1 line")).toBeInTheDocument();

    api.output.mockResolvedValue(page("first chunk\nsecond chunk", { total_lines: null }));
    await act(async () => { await vi.advanceTimersByTimeAsync(2_100); });

    expect(within(card).getByText("1 of 2 lines")).toBeInTheDocument();
    expect(within(card).getByText("second chunk")).toBeInTheDocument();
  });
});

describe("find in output", () => {
  const seven = "sign-in one\nline two\nline three\nline four\nline five\nline six\nsign-in seven\nsign-in again";

  it("marks every match with the current one filled, counts them and steps with Enter and Shift+Enter", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    expect(await within(card).findByText(/sign-in again/)).toBeInTheDocument();
    expect(within(card).queryByText(/sign-in one/)).not.toBeInTheDocument();

    const control = within(card).getByRole("button", { name: "Find in output" });
    await user.click(control);
    const field = within(card).getByRole("textbox", { name: "Find in output" });
    expect(field).toHaveFocus();
    await user.type(field, "sign-in");

    expect(within(card).getByText("1 of 3")).toBeInTheDocument();
    expect(card.querySelector(".cb-out")?.textContent).toBe(seven);
    expect(card.querySelectorAll("mark")).toHaveLength(3);
    expect(card.querySelectorAll("mark")[0]).toHaveAttribute("data-current");
    expect(control).toHaveAttribute("aria-pressed", "true");

    await user.keyboard("{Enter}");
    expect(within(card).getByText("2 of 3")).toBeInTheDocument();
    expect(card.querySelectorAll("mark")[1]).toHaveAttribute("data-current");

    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(within(card).getByText("1 of 3")).toBeInTheDocument();
    expect(card.querySelectorAll("mark")[0]).toHaveAttribute("data-current");

    await user.click(within(card).getByRole("button", { name: "Next match" }));
    expect(within(card).getByText("2 of 3")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Previous match" }));
    expect(within(card).getByText("1 of 3")).toBeInTheDocument();
  });

  it("says No matches, keeps the field's text and closes with Esc or Close find, returning focus", async () => {
    install([block({ command: "npm test" })], { b1: page(seven) });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    const control = within(card).getByRole("button", { name: "Find in output" });
    await user.click(control);
    const field = within(card).getByRole("textbox", { name: "Find in output" });
    await user.type(field, "absent");

    expect(within(card).getByText("No matches")).toBeInTheDocument();
    expect(field).toHaveValue("absent");
    expect(within(card).getByRole("button", { name: "Next match" })).toBeDisabled();
    expect(within(card).getByRole("button", { name: "Previous match" })).toBeDisabled();

    await user.keyboard("{Escape}");
    expect(within(card).queryByRole("textbox", { name: "Find in output" })).not.toBeInTheDocument();
    expect(control).toHaveFocus();

    await user.click(control);
    await user.type(within(card).getByRole("textbox", { name: "Find in output" }), "sign-in");
    await user.click(within(card).getByRole("button", { name: "Close find" }));
    expect(within(card).queryByRole("textbox", { name: "Find in output" })).not.toBeInTheDocument();
    expect(control).toHaveFocus();
    expect(card.querySelectorAll("mark")).toHaveLength(0);
  });

  it("draws the output's markup as text while marks are drawn around a match", async () => {
    install([block({ command: "echo" })], { b1: page('<img src=x onerror=alert(1)>\nsign-in <script>alert(2)</script>') });
    const user = userEvent.setup();
    const { container } = mount();
    const card = await screen.findByRole("group", { name: "Command echo" });
    await user.click(within(card).getByRole("button", { name: "Find in output" }));
    await user.type(within(card).getByRole("textbox", { name: "Find in output" }), "sign-in");

    expect(container.querySelector("img, script")).toBeNull();
    expect(card.querySelector(".cb-out")?.textContent).toContain("<img src=x onerror=alert(1)>");
    expect(card.querySelectorAll("mark")).toHaveLength(1);
    expect(card.querySelector("mark")?.textContent).toBe("sign-in");
  });
});

describe("filter and find controls", () => {
  it("are not offered on a full-screen program's block", async () => {
    install([block({ command: "vim notes.md", fullscreen: true, output_bytes: 0 })]);
    mount();
    const card = await screen.findByRole("group", { name: "Command vim notes.md" });
    expect(within(card).queryByRole("button", { name: "Filter output" })).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Find in output" })).not.toBeInTheDocument();
  });

  it("opens and expands a collapsed block from its header control", async () => {
    install([block({ command: "npm test" })], { b1: page("one\ntwo") });
    const user = userEvent.setup();
    mount();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    await user.click(within(card).getByRole("button", { name: "Collapse" }));
    await user.click(within(card).getByRole("button", { name: "Filter output" }));
    expect(within(card).getByRole("textbox", { name: "Filter output" })).toHaveFocus();
    expect(card.querySelector(".cb-out")).toBeInTheDocument();
  });
});
