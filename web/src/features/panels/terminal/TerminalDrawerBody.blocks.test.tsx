import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { wire } from "../../../wire.test-support";
import type { TerminalInfo } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { setBlocksApi, type CommandBlock } from "./blocks/blocksContract";
import { TerminalDrawerBody } from "./TerminalDrawerBody";
import { registerTerminal, terminalKey } from "./terminalControls";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId }: { terminalId?: string }) => <div>Output of {terminalId ?? "main"}</div> }));

let mock: MockApi;
const blockReads = () => wire().filter((call) => /\/blocks$/.test(call.path));
const terminal: TerminalInfo = { id: "t2", thread_id: "th", title: "npm test", group: "g2", direction: null, alive: false, exit_code: 0, clients: 1, script: null, created_at: new Date().toISOString() };
const block: CommandBlock = {
  id: "b1", command: "npm test", started_at: new Date(Date.now() - 5000).toISOString(), ended_at: new Date().toISOString(), exit_code: 0, cwd: null,
  output_bytes: 3, truncated: false, dropped_lines: 0, fullscreen: false,
};
const controls = () => ({ clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });

beforeEach(() => {
  mock = installMockFetch();
  mock.terminals.seed("th", [terminal]);
  useComposerChips.setState({ chips: {} });
  useUi.setState({ terminalOpen: true, toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
});
afterEach(() => {
  setBlocksApi(null);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("command blocks in the drawer", () => {
  it("leaves the terminal unchanged when the service answers an empty list", async () => {
    mock.terminals.setBlocks("th", "t2", []);
    setBlocksApi(null);
    const user = userEvent.setup();
    const { container } = render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);

    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await waitFor(() => expect(blockReads()).not.toHaveLength(0));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

    expect(blockReads()[0]?.path).toBe("/api/threads/th/terminals/t2/blocks");
    expect(screen.getByText("Output of t2")).toBeInTheDocument();
    expect(container.querySelector(".cb-list")).toBeNull();
    expect(screen.queryByRole("group", { name: "Command blocks" })).not.toBeInTheDocument();
    expect(screen.queryByText("Commands you run here appear as blocks")).not.toBeInTheDocument();
    expect(screen.queryByText("Couldn't read this terminal's blocks.")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
    expect(useUi.getState().toasts).toEqual([]);
  });

  it("shows the first command of a terminal that answered an empty list, by reading again while idle", async () => {
    mock.terminals.setBlocks("th", "t2", [], {});
    setBlocksApi(null);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);

    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await waitFor(() => expect(blockReads()).toHaveLength(1));
    expect(screen.queryByRole("group", { name: "Command blocks" })).not.toBeInTheDocument();

    // The command ran after that first read.
    mock.terminals.setBlocks("th", "t2", [block], { b1: "ok\n" });
    await waitFor(() => expect(blockReads().length).toBeGreaterThan(1), { timeout: 7000 });
    expect(await screen.findByRole("group", { name: "Command blocks" })).toBeInTheDocument();
  }, 12000);

  it("draws the blocks of the shown terminal, and Run again types the line into that terminal's input without a newline", async () => {
    mock.terminals.setBlocks("th", "t2", [block], { b1: "ok\n" });
    setBlocksApi(null);
    const mine = controls();
    const other = controls();
    const stop = [registerTerminal(terminalKey("th", "t2"), mine), registerTerminal(terminalKey("th", undefined), other)];
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await user.click(await screen.findByRole("button", { name: "Run again" }));
    expect(blockReads()[0]?.path).toBe("/api/threads/th/terminals/t2/blocks");
    expect(mine.write).toHaveBeenCalledTimes(1);
    expect(mine.write).toHaveBeenCalledWith("npm test");
    expect(other.write).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText("ok")).toBeInTheDocument());
    stop.forEach((off) => off());
  });

  it("offers no Run again in Watch only, where the terminal takes no input", async () => {
    mock.terminals.setBlocks("th", "t2", [block], { b1: "" });
    setBlocksApi(null);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await user.click(screen.getByRole("button", { name: "Watch only" }));
    expect(await screen.findByRole("button", { name: "Run again" })).toBeDisabled();
  });
});
