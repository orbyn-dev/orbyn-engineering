import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../../../api/client";
import { useConnection } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { registerTerminal } from "../terminalControls";
import { recordCommand } from "./history";
import { TerminalInput } from "./TerminalInput";

vi.mock("../../../../api/client", async (original) => {
  const real = await original<{ api: object }>();
  return { ...real, api: { ...real.api, startRun: vi.fn(), enqueue: vi.fn(), cancelRun: vi.fn(), removeQueued: vi.fn() } };
});
vi.mock("./useInputContext", async (original) => {
  const real = await original<typeof import("./useInputContext")>();
  return { ...real, usePathListing: () => ({ listDir: () => null, state: "idle" }) };
});

const KEY = "th:main";
const write = vi.fn();
let stop = () => undefined as void;

const setup = (over: { auth?: "logged_in" | "logged_out"; active?: boolean } = {}) => {
  useWorkspace.setState({
    threads: { th: { id: "th", project_id: "p", engine: "claude", model: "Sonnet", active_run: over.active ? { id: "r0" } : null } },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: over.auth ?? "logged_in" }] },
    refreshThread: vi.fn(() => Promise.resolve()),
  } as never);
  render(<TerminalInput threadId="th" keyOf={KEY} />);
  return { field: screen.getByLabelText("Terminal input") as HTMLTextAreaElement, user: userEvent.setup() };
};

beforeEach(() => {
  write.mockReset();
  vi.mocked(api.startRun).mockReset().mockResolvedValue({ id: "r1" } as never);
  vi.mocked(api.cancelRun).mockReset().mockResolvedValue(undefined as never);
  vi.mocked(api.enqueue).mockReset().mockResolvedValue({ id: "q1" } as never);
  useConnection.setState({ offline: false, serviceDown: false });
  stop = registerTerminal(KEY, { write, clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
});
afterEach(() => stop());

describe("terminal input", () => {
  it("runs a command in the shell with one write and sends nothing to the engine", async () => {
    const { field, user } = setup();
    await user.type(field, "git status{Enter}");
    expect(write).toHaveBeenCalledExactlyOnceWith("git status\r");
    expect(api.startRun).not.toHaveBeenCalled();
    expect(field.value).toBe("");
  });
  it("says where a sentence goes before it is sent, then sends it as a prompt and runs nothing in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "why is the build failing?");
    expect(screen.getByText(/prompt to Claude \(Sonnet\)/)).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(api.startRun).toHaveBeenCalledExactlyOnceWith("th", "why is the build failing?"));
    expect(write).not.toHaveBeenCalled();
    expect(await screen.findByText(/Sent to Claude as a prompt/)).toBeTruthy();
  });
  it("sends once however many times Enter is pressed", async () => {
    const { field } = setup();
    fireEvent.change(field, { target: { value: "how do I undo a commit?" } });
    await act(async () => {
      fireEvent.keyDown(field, { key: "Enter" });
      fireEvent.keyDown(field, { key: "Enter" });
    });
    expect(api.startRun).toHaveBeenCalledTimes(1);
  });
  it("queues behind a running turn", async () => {
    const { field, user } = setup({ active: true });
    await user.type(field, "why is it slow?{Enter}");
    await waitFor(() => expect(api.enqueue).toHaveBeenCalledWith("th", "why is it slow?", []));
    expect(api.startRun).not.toHaveBeenCalled();
  });
  it("Undo withdraws the prompt and puts the text back", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(api.cancelRun).toHaveBeenCalledWith("r1"));
    await waitFor(() => expect(field.value).toBe("why is it slow?"));
  });
  it("keeps an unclear line in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "make sure the tests pass{Enter}");
    expect(write).toHaveBeenCalledWith("make sure the tests pass\r");
    expect(api.startRun).not.toHaveBeenCalled();
  });
  it("Esc keeps a suggested prompt in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Escape}");
    expect(screen.getByText(/Nothing is sent to Claude/)).toBeTruthy();
    await user.keyboard("{Enter}");
    expect(write).toHaveBeenCalledWith("why is it slow?\r");
    expect(api.startRun).not.toHaveBeenCalled();
  });
  it("never sends a secret to the engine, even in Engine mode", async () => {
    const { field, user } = setup();
    await user.click(screen.getByRole("button", { name: "Engine" }));
    await user.type(field, "check export OPENAI_API_KEY=abc123 please{Enter}");
    expect(api.startRun).not.toHaveBeenCalled();
    expect(write).not.toHaveBeenCalled();
    expect(screen.getByText(/Looks like a secret/)).toBeTruthy();
  });
  it("asks twice before sending something that looks like a command to the engine", async () => {
    const { field, user } = setup();
    await user.click(screen.getByRole("button", { name: "Engine" }));
    await user.type(field, "ls -la | grep x{Enter}");
    expect(api.startRun).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(api.startRun).toHaveBeenCalledTimes(1));
    expect(write).not.toHaveBeenCalled();
  });
  it("does not run a question in the shell when the engine is signed out, and says so", async () => {
    const { field, user } = setup({ auth: "logged_out" });
    await user.type(field, "why is it slow?{Enter}");
    expect(write).not.toHaveBeenCalled();
    expect(api.startRun).not.toHaveBeenCalled();
    expect(screen.getByText(/not signed in/)).toBeTruthy();
  });
  it("does not run a question in the shell when offline", async () => {
    useConnection.setState({ offline: true });
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    expect(write).not.toHaveBeenCalled();
    expect(screen.getByText(/can't be reached/)).toBeTruthy();
  });
  it("holds a long paste aside and runs it only on a second Enter", async () => {
    const { field, user } = setup();
    fireEvent.paste(field, { clipboardData: { getData: () => "a\nb\nc\nd\ne\nf" } });
    expect(screen.getByText(/Pasted 6 lines/)).toBeTruthy();
    await user.type(field, "{Enter}");
    expect(write).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(write).toHaveBeenCalledWith("a\rb\rc\rd\re\rf\r");
  });
  it("shows history as text, masked, and puts the real command in the line", async () => {
    recordCommand(KEY, "echo <img src=x onerror=alert(1)>");
    const { user, field } = setup();
    await user.click(screen.getByRole("button", { name: /History/ }));
    const row = await screen.findByText("echo <img src=x onerror=alert(1)>");
    expect(row.closest("li")?.querySelector("img")).toBeNull();
    await user.keyboard("{Enter}");
    expect(field.value).toBe("echo <img src=x onerror=alert(1)>");
  });
});
