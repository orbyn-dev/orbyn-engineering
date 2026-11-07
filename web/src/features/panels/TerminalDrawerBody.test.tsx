import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { request } from "../../api/client";
import type { TerminalInfo } from "../../api/types";
import { useUi } from "../../state/ui";
import { useComposerChips } from "../thread/composerChipStore";
import { TerminalBar, TerminalDrawerBody } from "./TerminalDrawerBody";
import { registerTerminal, terminalKey } from "./terminalControls";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId }: { terminalId?: string }) => <div>Output of {terminalId ?? "main"}</div> }));
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
  useUi.setState({ terminalOpen: true });
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
    const stop = registerTerminal(terminalKey("th", "t2"), { clear, interrupt, output: () => "" });
    const user = await show([terminal({})]);
    expect(screen.getByText(/^Running · 1m 0\ds$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(clear).toHaveBeenCalled();
    expect(interrupt).toHaveBeenCalled();
    stop();
  });

  it("says a command failed with its code, sends the output to the agent and runs it again", async () => {
    const stop = registerTerminal(terminalKey("th", "t2"), { clear: vi.fn(), interrupt: vi.fn(), output: () => "1 failed | 47 passed" });
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

  it("offers Split, Clear, Copy and Watch from a tab's right-click menu, and End process only while it runs", async () => {
    const user = await show([terminal({})]);
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name: /npm test/ }) });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Split right", "Split down", "Clear output", "Copy all output", "Watch only", "End process"]);
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

  it("is not there when nothing runs", async () => {
    serve([terminal({ alive: false })]);
    render(<TerminalBar threadId="th" onOpen={() => undefined} />);
    await waitFor(() => expect(mock).toHaveBeenCalled());
    expect(screen.queryByRole("group", { name: "Terminal" })).not.toBeInTheDocument();
  });
});
