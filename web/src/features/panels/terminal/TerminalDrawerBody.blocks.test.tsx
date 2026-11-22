import { act, render, screen, waitFor } from "@testing-library/react";
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
  vi.unstubAllGlobals();
});

describe("command blocks in the drawer", () => {
  it("leaves the terminal unchanged when the real client answers an empty list", async () => {
    const fetching = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({
      blocks: [],
      next_before: null,
      limits: { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 },
    }), { status: 200 }));
    vi.stubGlobal("fetch", fetching);
    setBlocksApi(null);
    const user = userEvent.setup();
    const { container } = render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);

    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await waitFor(() => expect(fetching).toHaveBeenCalled());
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

    expect(String(fetching.mock.calls[0]?.[0])).toBe("/api/threads/th/terminals/t2/blocks");
    expect(screen.getByText("Output of t2")).toBeInTheDocument();
    expect(container.querySelector(".cb-list")).toBeNull();
    expect(screen.queryByRole("group", { name: "Command blocks" })).not.toBeInTheDocument();
    expect(screen.queryByText("Commands Orbyn runs appear here as blocks")).not.toBeInTheDocument();
    expect(screen.queryByText("Couldn't read this terminal's blocks.")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
    expect(useUi.getState().toasts).toEqual([]);
  });

  it("shows the first command of a terminal that answered an empty list, by reading again while idle", async () => {
    const answer = (blocks: CommandBlock[]) => new Response(JSON.stringify({
      blocks, next_before: null, limits: { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 },
    }), { status: 200 });
    const fetching = vi.fn()
      .mockResolvedValueOnce(answer([]))
      .mockImplementation(async () => answer([block]));
    vi.stubGlobal("fetch", fetching);
    setBlocksApi(null);
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);

    await user.click(await screen.findByRole("tab", { name: /npm test/ }));
    await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("group", { name: "Command blocks" })).not.toBeInTheDocument();

    await waitFor(() => expect(fetching.mock.calls.length).toBeGreaterThan(1), { timeout: 7000 });
    expect(await screen.findByRole("group", { name: "Command blocks" })).toBeInTheDocument();
  }, 12000);

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
