import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { TerminalInfo } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { setBlocksApi, type CommandBlock } from "./blocks/blocksContract";
import { TerminalDrawerBody } from "./TerminalDrawerBody";
import { registerTerminal, terminalKey } from "./terminalControls";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId }: { terminalId?: string }) => <div>Output of {terminalId ?? "main"}</div> }));
vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));
vi.mock("../../../product/composer/runProjectAction", () => ({ runProjectAction: vi.fn(() => Promise.resolve()) }));

const mock = vi.mocked(request);
const terminal: TerminalInfo = { id: "t2", thread_id: "th", title: "npm test", group: "g2", direction: null, alive: false, exit_code: 0, clients: 1, script: null, created_at: new Date().toISOString() };
const block: CommandBlock = {
  id: "b1", command: "npm test", started_at: new Date(Date.now() - 5000).toISOString(), ended_at: new Date().toISOString(), exit_code: 0, cwd: null,
  output_bytes: 3, truncated: false, dropped_lines: 0, fullscreen: false,
};
const limits = { max_blocks: 1, max_block_bytes: 1, max_total_bytes: 1 };
const controls = () => ({ clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });

beforeEach(() => {
  mock.mockReset();
  mock.mockImplementation((method: string, path: string) => {
    if (method === "GET" && path.endsWith("/terminals")) return Promise.resolve([terminal]) as never;
    if (method === "GET" && path.endsWith("/scripts")) return Promise.resolve({ scripts: [] }) as never;
    return Promise.resolve(undefined) as never;
  });
  useComposerChips.setState({ chips: {} });
  useUi.setState({ terminalOpen: true, toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
});
afterEach(() => {
  setBlocksApi(null);
  vi.restoreAllMocks();
});

describe("command blocks in the drawer", () => {
  it("leaves the terminal as it was while no client is installed, and asks nobody for blocks", async () => {
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await screen.findByRole("tab", { name: /npm test/ });
    expect(screen.queryByRole("group", { name: "Command blocks" })).not.toBeInTheDocument();
    expect(mock.mock.calls.every(([, path]) => !String(path).includes("blocks"))).toBe(true);
  });

  it("draws the blocks of the shown terminal, and Run again types the line into that terminal's input without a newline", async () => {
    const list = vi.fn().mockResolvedValue({ blocks: [block], next_before: null, limits });
    setBlocksApi({ list, output: vi.fn().mockResolvedValue({ text: "ok", next: null, total_lines: 1, dropped_lines: 0 }) });
    const mine = controls();
    const other = controls();
    const stop = [registerTerminal(terminalKey("th", "t2"), mine), registerTerminal(terminalKey("th", undefined), other)];
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await user.click(await screen.findByRole("button", { name: "Run again" }));
    expect(list).toHaveBeenCalledWith({ threadId: "th", terminalId: "t2" }, {}, expect.anything());
    expect(mine.write).toHaveBeenCalledTimes(1);
    expect(mine.write).toHaveBeenCalledWith("npm test");
    expect(other.write).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText("ok")).toBeInTheDocument());
    stop.forEach((off) => off());
  });

  it("offers no Run again in Watch only, where the terminal takes no input", async () => {
    setBlocksApi({
      list: vi.fn().mockResolvedValue({ blocks: [block], next_before: null, limits }),
      output: vi.fn().mockResolvedValue({ text: "", next: null, total_lines: 0, dropped_lines: 0 }),
    });
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await user.click(screen.getByRole("button", { name: "Watch only" }));
    expect(await screen.findByRole("button", { name: "Run again" })).toBeDisabled();
  });
});
