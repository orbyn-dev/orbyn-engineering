import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { WireThreadEvent } from "../../../../api/types";
import { useConnection } from "../../../../product/state/connection";
import { useThreadLogs } from "../../../../product/state/threadLogs";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { holdFetch, wire } from "../../../../wire.test-support";
import { useExplorer } from "../../../workspace";
import { registerTerminal } from "../terminalControls";
import { recordCommand } from "./history";
import { offerProposal, useProposalSource } from "./proposal";
import { TerminalInput } from "./TerminalInput";

// Idle threads of the mock service: two of the app project and one of the site project.
const THREAD = "t-done";
const SIBLING = "t-tools";
const ELSEWHERE = "t-unmanaged";
const KEY = `${THREAD}:main`;
let mock: MockApi;
const prompts = () => wire().filter((call) => call.method === "POST" && call.path === `/api/threads/${THREAD}/runs`);
const cancels = () => wire().filter((call) => call.method === "POST" && /^\/api\/runs\/[^/]+\/cancel$/.test(call.path));
const removals = () => wire().filter((call) => call.method === "DELETE" && call.path.startsWith("/api/queue/"));
const toasts = () => useUi.getState().toasts.map((toast) => toast.text);
/** The run the mock service started for the editor's last prompt. */
const startedRun = () => mock.threads.find((thread) => thread.id === THREAD)?.active_run?.id ?? "";
const write = vi.fn();
let stop = () => undefined as void;

const setup = (over: { auth?: "logged_in" | "logged_out"; active?: boolean; signIn?: string | null } = {}) => {
  useWorkspace.setState({
    threads: {
      [THREAD]: { id: THREAD, project_id: "p-app", engine: "claude", model: "Sonnet", active_run: over.active ? { id: "r0" } : null },
      [SIBLING]: { id: SIBLING, project_id: "p-app", engine: "claude", model: "Sonnet", active_run: null },
      [ELSEWHERE]: { id: ELSEWHERE, project_id: "p-site", engine: "claude", model: "Sonnet", active_run: null },
    },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: over.auth ?? "logged_in", sign_in_command: over.signIn === undefined ? "claude login" : over.signIn }] },
  } as never);
  render(<TerminalInput threadId={THREAD} keyOf={KEY} />);
  return { field: screen.getByLabelText("Terminal input") as HTMLTextAreaElement, user: userEvent.setup() };
};

const answer = (text: string, runId = "r1") =>
  act(() => {
    useThreadLogs.setState({ logs: { [THREAD]: { items: [{ kind: "message", role: "assistant", text, done: true, runId, key: "m1", seq: 2, ts: "" }] } } } as never);
  });

beforeEach(() => {
  write.mockReset();
  // A long step keeps the run the editor starts alive, so Undo has a run to stop.
  mock = installMockFetch({ stepMs: 60_000 });
  useUi.setState({ toasts: [] });
  // The folders listed for path completion are kept in the explorer's cache, which a test starts without.
  useExplorer.setState({ worktrees: {}, levels: {} });
  useThreadLogs.setState({ logs: {} });
  useProposalSource.getState().reset();
  useConnection.setState({ offline: false, serviceDown: false });
  stop = registerTerminal(KEY, { write, clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
});
afterEach(() => {
  stop();
  vi.unstubAllGlobals();
});

describe("Undo after the engine started", () => {
  it("says it stops the run, really cancels it and tells the person it did", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    expect(await screen.findByText(/Undo stops the run that started/)).toBeTruthy();
    const runId = startedRun();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(cancels().map((call) => call.path)).toEqual([`/api/runs/${runId}/cancel`]));
    await waitFor(() => expect(toasts()).toContain("Stopped the run that started. Your text is back in the line."));
    expect(field.value).toBe("why is it slow?");
  });
  it("says a queued message is only taken out of the queue", async () => {
    const { field, user } = setup({ active: true });
    await user.type(field, "why is it slow?{Enter}");
    expect(await screen.findByText(/Undo takes it out of the queue/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(removals()).toHaveLength(1));
    await waitFor(() => expect(toasts()).toContain("Took it out of the queue. Your text is back in the line."));
  });
  it("does not claim the run stopped when stopping failed", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    await screen.findByText(/Undo stops the run that started/);
    mock.fail("/api/runs/", 1, 500, "internal", "Something went wrong.");
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(toasts().some((text) => /^Couldn't stop the run/.test(text))).toBe(true));
    expect(toasts().some((text) => /^Stopped the run/.test(text))).toBe(false);
  });
});

describe("the service proposes a command (typed field)", () => {
  let nextSeq = 1;
  const propose = (command: string, note = "Lists the files changed. It does not change files.", runId = startedRun()) =>
    act(() => offerProposal(THREAD, runId, { command, note }));
  const arrived = (value: unknown, runId: string | null = startedRun()) => {
    const event = { seq: nextSeq++, thread_id: THREAD, run_id: runId, ts: "", event: { type: "CUSTOM", name: "orbyn.proposal", value } } as unknown as WireThreadEvent;
    act(() => useThreadLogs.getState().ingest(THREAD, [event]));
  };
  const asked = async () => {
    const view = setup();
    await view.user.type(view.field, "list the files that changed since main?{Enter}");
    await screen.findByText(/Sent to Claude/);
    return view;
  };

  it("draws nothing and parses no text when the service reports no field", async () => {
    const view = setup();
    await view.user.type(view.field, "list the files that changed since main?{Enter}");
    await screen.findByText(/Sent to Claude/);
    answer("Try this.\n<proposed_command>git status</proposed_command>");
    expect(document.querySelector(".term-proposal")).toBeNull();
    expect(write).not.toHaveBeenCalled();
  });
  it("shows the card for the orbyn.proposal event of the run the editor started", async () => {
    await asked();
    arrived({ command: "git status --short", note: "Lists the files changed." });
    expect(await screen.findByText("git status --short")).toBeTruthy();
    expect(screen.getByText("Lists the files changed.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Run" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
    expect(write).not.toHaveBeenCalled();
  });
  it("ignores a proposal recorded for another run", async () => {
    await asked();
    arrived({ command: "git status" }, "r99");
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("drops a malformed value whole", async () => {
    await asked();
    for (const value of [null, "git status", 5, [], {}, { command: 5 }, { command: "git status", note: 5 }, { note: "no command" }]) arrived(value);
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it.each([
    ["two lines", "printf 'a\nb'"],
    ["a carriage return", "printf 'a\rb'"],
    ["a control character", "printf '\u001b[2J'"],
    ["an over-long command", `echo ${"a".repeat(4096)}`],
  ])("drops a command with %s", async (_name, command) => {
    await asked();
    arrived({ command });
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("keeps a queued prompt with no card, because its run is not known", async () => {
    const view = setup({ active: true });
    await view.user.type(view.field, "list the files that changed since main?{Enter}");
    await screen.findByText(/Queued behind/);
    arrived({ command: "git status" }, "r2");
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("turns the card off for a test with proposalSource none", async () => {
    useProposalSource.getState().setSource("none");
    await asked();
    arrived({ command: "git status" });
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("ignores command text in the model's answer", async () => {
    await asked();
    answer("Try this.\n<proposed_command>rm -rf x</proposed_command>");
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("shows the command as plain text with Run, Edit and Cancel, and writes nothing by itself", async () => {
    await asked();
    propose("git diff --name-only main...HEAD");
    expect(await screen.findByText("git diff --name-only main...HEAD")).toBeTruthy();
    expect(screen.getByText(/It does not run until you press Run/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Run" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
    expect(write).not.toHaveBeenCalled();
  });
  it("Run is one click and one write, however fast it is clicked", async () => {
    await asked();
    propose("git status");
    const run = await screen.findByRole("button", { name: "Run" });
    fireEvent.click(run);
    fireEvent.click(run);
    expect(write).toHaveBeenCalledExactlyOnceWith("git status\r");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Run" })).toBeNull());
  });
  it("Edit puts the command in the line and runs nothing", async () => {
    const { field, user } = await asked();
    propose("git status");
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    expect(field.value).toBe("git status");
    expect(write).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });
  it("Cancel dismisses the card and runs nothing", async () => {
    const { user } = await asked();
    propose("git status");
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(screen.queryByText("git status")).toBeNull();
    expect(write).not.toHaveBeenCalled();
  });
  it("shows markup as text", async () => {
    await asked();
    propose("echo <img src=x onerror=alert(1)>", "<b>bold</b> says hi");
    const code = await screen.findByText("echo <img src=x onerror=alert(1)>");
    expect(code.closest(".term-proposal")?.querySelector("img, p b")).toBeNull();
    expect(screen.getByText("<b>bold</b> says hi")).toBeTruthy();
  });
  it("never fills in or runs a secret-looking command the service reports", async () => {
    const { field } = await asked();
    arrived({ command: "export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx" });
    expect(await screen.findByText(/looks like a secret/i)).toBeTruthy();
    expect(screen.queryByText(/sk-abcdefghijklmnopqrstuvwx/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
    expect(field.value).toBe("");
    expect(write).not.toHaveBeenCalled();
  });
  it("ignores a proposal for some other run", async () => {
    await asked();
    propose("git status", "n", "r99");
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });
});

describe("path completion while the listing is read", () => {
  it("shows a quiet loading row only after a moment, and leaves the line alone", async () => {
    const slow = holdFetch("GET", /^\/api\/projects\/p-app\/worktrees\/[^/]+\/tree/);
    const { field, user } = setup();
    await user.type(field, "cat src/");
    expect(screen.queryByText(/Reading files/)).toBeNull();
    expect(await screen.findByText(/Reading files/)).toBeTruthy();
    expect(field.value).toBe("cat src/");
    await user.keyboard("{Enter}");
    expect(write).toHaveBeenCalledWith("cat src/\r");
    slow.release();
  });
  it("says when the folder could not be read", async () => {
    // The worktree root is listed first, then the folder the line names: both reads are refused.
    mock.fail("/api/projects/p-app/worktrees/p-app-main/tree", 2, 404, "not_found", "That worktree's folder is gone.");
    const { field, user } = setup();
    await user.type(field, "cat src/");
    expect(await screen.findByText(/Couldn't read this folder/)).toBeTruthy();
    expect(field.value).toBe("cat src/");
  });
  it("shows nothing for a word that is not a path", async () => {
    const slow = holdFetch("GET", /^\/api\/projects\/p-app\/worktrees\/[^/]+\/tree/);
    const { field, user } = setup();
    await user.type(field, "cat ");
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(screen.queryByText(/Reading files/)).toBeNull();
    slow.release();
  });
});

describe("Sign in and Try again", () => {
  it("Sign in copies the engine's own sign-in command, as the Welcome page does, and sends nothing", async () => {
    const { field, user } = setup({ auth: "logged_out" });
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    await user.type(field, "why is it slow?");
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    expect(writeText).toHaveBeenCalledExactlyOnceWith("claude login");
    await waitFor(() => expect(toasts()).toContain("Copied the Claude sign-in command. Run it in a terminal."));
    expect(prompts()).toHaveLength(0);
    expect(write).not.toHaveBeenCalled();
    expect(field.value).toBe("why is it slow?");
  });
  it("with no sign-in command, Sign in is a link to the providers settings", async () => {
    const { field, user } = setup({ auth: "logged_out", signIn: null });
    await user.type(field, "why is it slow?");
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toContain("providers");
  });
  it("Try again checks once per click and sends the line when the engine is back", async () => {
    useConnection.setState({ offline: true });
    // The device is reachable again by the time the service answers the check.
    const service = fetch;
    vi.stubGlobal("fetch", vi.fn((url: string, init?: RequestInit) => {
      if (url === "/api/engines/refresh") useConnection.setState({ offline: false });
      return service(url, init);
    }));
    const { field, user } = setup();
    await user.type(field, "why is it slow?");
    const again = screen.getByRole("button", { name: "Try again" });
    fireEvent.click(again);
    fireEvent.click(again);
    await waitFor(() => expect(prompts()).toHaveLength(1));
    expect(prompts()[0]?.body).toEqual({ prompt: "why is it slow?", purpose: "terminal_proposal" });
    expect(wire().filter((call) => call.path === "/api/engines/refresh")).toHaveLength(1);
    expect(write).not.toHaveBeenCalled();
  });
  it("Try again sends nothing and says so while the engine is still out of reach", async () => {
    useConnection.setState({ offline: true });
    const { field, user } = setup();
    await user.type(field, "why is it slow?");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(toasts().some((text) => /still can't be reached/.test(text))).toBe(true));
    expect(prompts()).toHaveLength(0);
    expect(field.value).toBe("why is it slow?");
  });
});

describe("history across the project", () => {
  it("Shift+Ctrl+R widens to this project's terminals, with how long ago each ran", async () => {
    recordCommand(KEY, "git status", Date.now() - 4 * 60_000);
    recordCommand(`${SIBLING}:main`, "npm test", Date.now() - 2 * 3_600_000);
    recordCommand(`${ELSEWHERE}:main`, "secret project cmd", Date.now());
    const { field, user } = setup();
    fireEvent.keyDown(field, { key: "R", ctrlKey: true });
    expect(await screen.findByText("This shell")).toBeTruthy();
    expect(screen.queryByText("npm test")).toBeNull();
    expect(screen.getByText("4 min ago")).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Search history" }), { key: "Escape" });
    fireEvent.keyDown(field, { key: "R", ctrlKey: true, shiftKey: true });
    expect(await screen.findByText("This project")).toBeTruthy();
    expect(screen.getByText("npm test")).toBeTruthy();
    expect(screen.getByText("2 hr ago")).toBeTruthy();
    expect(screen.queryByText("secret project cmd")).toBeNull();
    await user.click(screen.getByRole("button", { name: /This project/ }));
    expect(screen.queryByText("npm test")).toBeNull();
  });
});
