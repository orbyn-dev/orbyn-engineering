import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../../../api/client";
import { useConnection } from "../../../../product/state/connection";
import { useThreadLogs } from "../../../../product/state/threadLogs";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { registerTerminal } from "../terminalControls";
import { recordCommand } from "./history";
import { offerProposal, useProposalSource } from "./proposal";
import { TerminalInput } from "./TerminalInput";

vi.mock("../../../../api/client", async (original) => {
  const real = await original<{ api: object }>();
  return { ...real, api: { ...real.api, startRun: vi.fn(), enqueue: vi.fn(), cancelRun: vi.fn(), removeQueued: vi.fn(), refreshEngines: vi.fn() } };
});
const pathState = vi.hoisted(() => ({ state: "idle" as "idle" | "loading" | "failed" }));
vi.mock("./useInputContext", async (original) => {
  const real = await original<typeof import("./useInputContext")>();
  return { ...real, usePathListing: () => ({ listDir: () => null, state: pathState.state }) };
});

const KEY = "th:main";
const write = vi.fn();
const toast = vi.fn();
let stop = () => undefined as void;
let afterRefresh = () => undefined as void;

const setup = (over: { auth?: "logged_in" | "logged_out"; active?: boolean; signIn?: string | null } = {}) => {
  useWorkspace.setState({
    threads: {
      th: { id: "th", project_id: "p", engine: "claude", model: "Sonnet", active_run: over.active ? { id: "r0" } : null },
      th2: { id: "th2", project_id: "p", engine: "claude", model: "Sonnet", active_run: null },
      other: { id: "other", project_id: "q", engine: "claude", model: "Sonnet", active_run: null },
    },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: over.auth ?? "logged_in", sign_in_command: over.signIn === undefined ? "claude login" : over.signIn }] },
    refreshThread: vi.fn(() => Promise.resolve()),
    refreshEngines: vi.fn(() => Promise.resolve()),
  } as never);
  render(<TerminalInput threadId="th" keyOf={KEY} />);
  return { field: screen.getByLabelText("Terminal input") as HTMLTextAreaElement, user: userEvent.setup() };
};

const answer = (text: string, runId = "r1") =>
  act(() => {
    useThreadLogs.setState({ logs: { th: { items: [{ kind: "message", role: "assistant", text, done: true, runId, key: "m1", seq: 2, ts: "" }] } } } as never);
  });

beforeEach(() => {
  write.mockReset();
  toast.mockReset();
  pathState.state = "idle";
  afterRefresh = () => undefined;
  useUi.setState({ pushToast: toast } as never);
  useThreadLogs.setState({ logs: {} });
  useProposalSource.getState().reset();
  vi.mocked(api.startRun).mockReset().mockResolvedValue({ id: "r1" } as never);
  vi.mocked(api.cancelRun).mockReset().mockResolvedValue(undefined as never);
  vi.mocked(api.enqueue).mockReset().mockResolvedValue({ id: "q1" } as never);
  vi.mocked(api.removeQueued).mockReset().mockResolvedValue(undefined as never);
  vi.mocked(api.refreshEngines).mockReset().mockImplementation(async () => {
    afterRefresh();
    return [] as never;
  });
  useConnection.setState({ offline: false, serviceDown: false });
  stop = registerTerminal(KEY, { write, clear: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
});
afterEach(() => stop());

describe("Undo after the engine started", () => {
  it("says it stops the run, really cancels it and tells the person it did", async () => {
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    expect(await screen.findByText(/Undo stops the run that started/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(api.cancelRun).toHaveBeenCalledExactlyOnceWith("r1"));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Stopped the run that started. Your text is back in the line."));
    expect(field.value).toBe("why is it slow?");
  });
  it("says a queued message is only taken out of the queue", async () => {
    const { field, user } = setup({ active: true });
    await user.type(field, "why is it slow?{Enter}");
    expect(await screen.findByText(/Undo takes it out of the queue/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(api.removeQueued).toHaveBeenCalledWith("q1"));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Took it out of the queue. Your text is back in the line."));
  });
  it("does not claim the run stopped when stopping failed", async () => {
    vi.mocked(api.cancelRun).mockRejectedValue(new Error("gone"));
    const { field, user } = setup();
    await user.type(field, "why is it slow?{Enter}");
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith(expect.stringMatching(/^Couldn't stop the run/)));
    expect(toast).not.toHaveBeenCalledWith(expect.stringMatching(/^Stopped the run/));
  });
});

describe("the service proposes a command (typed field)", () => {
  const propose = (command: string, note = "Lists the files changed. It does not change files.", runId = "r1") =>
    act(() => offerProposal("th", runId, { command, note }));
  const asked = async () => {
    useProposalSource.getState().setSource("service");
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
    act(() => offerProposal("th", "r1", { command: "git status" }));
    expect(document.querySelector(".term-proposal")).toBeNull();
    expect(write).not.toHaveBeenCalled();
  });
  it("shows the command as plain text with Accept and Dismiss, and writes nothing by itself", async () => {
    await asked();
    propose("git diff --name-only main...HEAD");
    expect(await screen.findByText("git diff --name-only main...HEAD")).toBeTruthy();
    expect(screen.getByText(/Nothing runs from here/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Accept" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
    expect(write).not.toHaveBeenCalled();
  });
  it("ignores command text in the model's answer", async () => {
    await asked();
    answer("Try this.\n<proposed_command>rm -rf x</proposed_command>");
    expect(document.querySelector(".term-proposal")).toBeNull();
  });
  it("Accept puts the command in the line once and runs nothing", async () => {
    const { field } = await asked();
    propose("git status");
    const accept = await screen.findByRole("button", { name: "Accept" });
    fireEvent.click(accept);
    fireEvent.click(accept);
    expect(field.value).toBe("git status");
    expect(write).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Accept" })).toBeNull());
  });
  it("Dismiss removes the card and touches nothing", async () => {
    const { field, user } = await asked();
    propose("git status");
    await user.click(await screen.findByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("git status")).toBeNull();
    expect(field.value).toBe("");
    expect(write).not.toHaveBeenCalled();
  });
  it("shows markup as text", async () => {
    await asked();
    propose("echo <img src=x onerror=alert(1)>", "<b>bold</b> says hi");
    const code = await screen.findByText("echo <img src=x onerror=alert(1)>");
    expect(code.closest(".term-proposal")?.querySelector("img, p b")).toBeNull();
    expect(screen.getByText("<b>bold</b> says hi")).toBeTruthy();
  });
  it("never fills in a command that looks like a secret", async () => {
    const { field } = await asked();
    propose("export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx");
    expect(await screen.findByText(/looks like a secret/i)).toBeTruthy();
    expect(screen.queryByText(/sk-abcdefghijklmnopqrstuvwx/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
    expect(field.value).toBe("");
    expect(write).not.toHaveBeenCalled();
  });
  it("ignores a proposal for some other run", async () => {
    await asked();
    propose("git status", "n", "r99");
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });
});

describe("path completion while the listing is read", () => {
  it("shows a quiet loading row only after a moment, and leaves the line alone", async () => {
    pathState.state = "loading";
    const { field, user } = setup();
    await user.type(field, "cat src/");
    expect(screen.queryByText(/Reading files/)).toBeNull();
    expect(await screen.findByText(/Reading files/)).toBeTruthy();
    expect(field.value).toBe("cat src/");
    await user.keyboard("{Enter}");
    expect(write).toHaveBeenCalledWith("cat src/\r");
  });
  it("says when the folder could not be read", async () => {
    pathState.state = "failed";
    const { field, user } = setup();
    await user.type(field, "cat src/");
    expect(await screen.findByText(/Couldn't read this folder/)).toBeTruthy();
    expect(field.value).toBe("cat src/");
  });
  it("shows nothing for a word that is not a path", async () => {
    pathState.state = "loading";
    const { field, user } = setup();
    await user.type(field, "cat ");
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(screen.queryByText(/Reading files/)).toBeNull();
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
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Copied the Claude sign-in command. Run it in a terminal."));
    expect(api.startRun).not.toHaveBeenCalled();
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
    afterRefresh = () => useConnection.setState({ offline: false });
    const { field, user } = setup();
    await user.type(field, "why is it slow?");
    const again = screen.getByRole("button", { name: "Try again" });
    fireEvent.click(again);
    fireEvent.click(again);
    await waitFor(() => expect(api.startRun).toHaveBeenCalledExactlyOnceWith("th", "why is it slow?"));
    expect(api.refreshEngines).toHaveBeenCalledTimes(1);
    expect(write).not.toHaveBeenCalled();
  });
  it("Try again sends nothing and says so while the engine is still out of reach", async () => {
    useConnection.setState({ offline: true });
    const { field, user } = setup();
    await user.type(field, "why is it slow?");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith(expect.stringMatching(/still can't be reached/)));
    expect(api.startRun).not.toHaveBeenCalled();
    expect(field.value).toBe("why is it slow?");
  });
});

describe("history across the project", () => {
  it("Shift+Ctrl+R widens to this project's terminals, with how long ago each ran", async () => {
    recordCommand(KEY, "git status", Date.now() - 4 * 60_000);
    recordCommand("th2:main", "npm test", Date.now() - 2 * 3_600_000);
    recordCommand("other:main", "secret project cmd", Date.now());
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
