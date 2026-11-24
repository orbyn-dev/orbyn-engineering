import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { useConnection } from "../../../../product/state/connection";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { wire } from "../../../../wire.test-support";
import { registerTerminal } from "../terminalControls";
import { recordCommand } from "./history";
import { TerminalInput } from "./TerminalInput";

// An idle thread of the mock service, which answers the runs, the queue and the cancel the editor sends.
const THREAD = "t-done";
const KEY = `${THREAD}:main`;
let mock: MockApi;
const prompts = () => wire().filter((call) => call.method === "POST" && call.path === `/api/threads/${THREAD}/runs`);
const queued = () => wire().filter((call) => call.method === "POST" && call.path === `/api/threads/${THREAD}/queue`);
const cancels = () => wire().filter((call) => call.method === "POST" && /^\/api\/runs\/[^/]+\/cancel$/.test(call.path));
const write = vi.fn();
let stop = () => undefined as void;

const setup = (over: { auth?: "logged_in" | "logged_out"; active?: boolean } = {}) => {
  useWorkspace.setState({
    threads: { [THREAD]: { id: THREAD, project_id: "p-app", engine: "claude", model: "Sonnet", active_run: over.active ? { id: "r0" } : null } },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: over.auth ?? "logged_in" }] },
  } as never);
  render(<TerminalInput threadId={THREAD} keyOf={KEY} />);
  return { field: screen.getByLabelText("Terminal input") as HTMLTextAreaElement, user: userEvent.setup() };
};

beforeEach(() => {
  write.mockReset();
  // A long step keeps the run the editor starts alive, so Undo has a run to stop.
  mock = installMockFetch({ stepMs: 60_000 });
  useUi.setState({ toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
  stop = registerTerminal(KEY, { write, clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
});
afterEach(() => {
  stop();
  vi.unstubAllGlobals();
});

describe("terminal input", () => {
  it("runs a command in the shell with one write and sends nothing to the engine", async () => {
    const { field, user } = setup();
    await user.type(field, "git status{Enter}");
    expect(write).toHaveBeenCalledExactlyOnceWith("git status\r");
    expect(prompts()).toHaveLength(0);
    expect(field.value).toBe("");
  });
  it("says where a sentence goes before it is sent, then sends it as a prompt and runs nothing in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "why is the build failing?");
    expect(screen.getByText(/prompt to Claude \(Sonnet\)/)).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(prompts()).toHaveLength(1));
    expect(prompts()[0]?.body).toEqual({ prompt: "why is the build failing?", purpose: "terminal_proposal" });
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
    expect(prompts()).toHaveLength(1);
  });
  it("queues behind a running turn", async () => {
    const { field, user } = setup({ active: true });
    await user.type(field, "why is it slow?{Enter}");
    await waitFor(() => expect(queued()).toHaveLength(1));
    expect(queued()[0]?.body).toEqual({ prompt: "why is it slow?", attachment_ids: [] });
    expect(prompts()).toHaveLength(0);
  });
  it("Undo withdraws the prompt and puts the text back", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    await waitFor(() => expect(prompts()).toHaveLength(1));
    const runId = mock.threads.find((thread) => thread.id === THREAD)?.active_run?.id;
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(cancels().map((call) => call.path)).toEqual([`/api/runs/${runId}/cancel`]));
    await waitFor(() => expect(field.value).toBe("why is it slow?"));
  });
  it("keeps an unclear line in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "make sure the tests pass{Enter}");
    expect(write).toHaveBeenCalledWith("make sure the tests pass\r");
    expect(prompts()).toHaveLength(0);
  });
  it("Esc keeps a suggested prompt in the shell", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Escape}");
    expect(screen.getByText(/Nothing is sent to Claude/)).toBeTruthy();
    await user.keyboard("{Enter}");
    expect(write).toHaveBeenCalledWith("why is it slow?\r");
    expect(prompts()).toHaveLength(0);
  });
  it("never sends a secret to the engine, even in Engine mode", async () => {
    const { field, user } = setup();
    await user.click(screen.getByRole("button", { name: "Engine" }));
    await user.type(field, "check export OPENAI_API_KEY=abc123 please{Enter}");
    expect(prompts()).toHaveLength(0);
    expect(write).not.toHaveBeenCalled();
    expect(screen.getByText(/Looks like a secret/)).toBeTruthy();
  });
  it("asks twice before sending something that looks like a command to the engine", async () => {
    const { field, user } = setup();
    await user.click(screen.getByRole("button", { name: "Engine" }));
    await user.type(field, "ls -la | grep x{Enter}");
    expect(prompts()).toHaveLength(0);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(prompts()).toHaveLength(1));
    expect(prompts()[0]?.body).toEqual({ prompt: "ls -la | grep x", purpose: "terminal_proposal" });
    expect(write).not.toHaveBeenCalled();
  });
  it("does not run a question in the shell when the engine is signed out, and says so", async () => {
    const { field, user } = setup({ auth: "logged_out" });
    await user.type(field, "why is it slow?{Enter}");
    expect(write).not.toHaveBeenCalled();
    expect(prompts()).toHaveLength(0);
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
