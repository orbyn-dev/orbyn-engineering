import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, api } from "../../../api/client";
import type { EngineModels, EngineStatus, Run, Thread } from "../../../api/types";
import { useModelStore } from "../../../product/state/modelStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { setCursorThreadApi } from "../../cursorthread";
import { createCursorThreadMock } from "../../../../mock/cursorThread";
import { Composer } from "./Composer";

// An engine that never asks before acting, as the service words its list (a guess until the service slice merges). No test names it.
const cursor = {
  id: "cursor", name: "Cursor", installed: true,
  capabilities: { managed: true, permission_modes: ["no_approvals"], approvals: false, skill_delivery: [] },
} as unknown as EngineStatus;
const thread: Thread = {
  id: "t1", project_id: "p", title: "Thread", engine: "cursor", model: "auto", cwd: "/r", worktree: null, skill_profile: null,
  scratch: false, active_run: null, last_run_state: null, created_at: "", updated_at: "",
  lifecycle: { permission_mode: "auto" },
} as unknown as Thread;
const catalog: EngineModels = { engine: "cursor", source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [{ id: "auto", label: "Auto", description: null, is_default: true, custom: false, options: [] }], options: [], agents: [], output_styles: [] };

const refusal = (code: string, message = "No.") => new ApiFailure(code, message, 409);
const field = () => screen.getByRole("textbox");

beforeEach(() => {
  useUi.setState({ drafts: { t1: "Run the tests." } });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false }, engines: { data: [cursor], error: null, loading: false } });
  useModelStore.setState({ byEngine: { cursor: catalog }, loading: {}, errors: {} });
  vi.spyOn(api, "queue").mockResolvedValue([]);
  vi.spyOn(api, "models").mockResolvedValue(catalog);
});
afterEach(() => {
  setCursorThreadApi(null);
  vi.restoreAllMocks();
});

const setup = () => render(<Composer thread={thread} engine={cursor} showContext />);

describe("the first-run notice", () => {
  it("draws the notice on engine_notice_required, dims Send and keeps the message", async () => {
    const start = vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_notice_required"));
    setup();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const card = await screen.findByRole("group", { name: "Before the first Cursor run" });
    expect(card).toHaveTextContent("Cursor does not ask before acting. Its own sandbox applies.");
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Choose another engine" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(field()).toHaveValue("Run the tests.");
    expect(start).toHaveBeenCalledTimes(1);
  });

  it("Continue is one write, even when pressed twice, then the message goes out and the card is gone", async () => {
    const start = vi.spyOn(api, "startRun").mockRejectedValueOnce(refusal("engine_notice_required")).mockResolvedValue({ id: "r1" } as Run);
    const mock = createCursorThreadMock({ delayMs: 20 });
    setCursorThreadApi(mock);
    setup();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const go = await screen.findByRole("button", { name: "Continue" });
    fireEvent.click(go);
    fireEvent.click(go);
    await waitFor(() => expect(start).toHaveBeenCalledTimes(2));
    // The notice is once per project: the thread's project id goes with it.
    expect(mock.writes.acknowledge).toEqual([{ project_id: "p", engine: "cursor" }]);
    expect(screen.queryByRole("group", { name: "Before the first Cursor run" })).toBeNull();
    expect(start.mock.calls[1]?.[1]).toBe("Run the tests.");
  });

  it("keeps the card and the message when the write fails, and says so", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_notice_required"));
    setCursorThreadApi(null);
    setup();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save that. This service does not offer that yet. Your message is kept.");
    expect(screen.getByRole("group", { name: "Before the first Cursor run" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    expect(field()).toHaveValue("Run the tests.");
  });

  it("Choose another engine opens the model picker and the card steps aside, and comes back if it closes with the engine still chosen", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_notice_required"));
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Send" }));
    await user.click(await screen.findByRole("button", { name: "Choose another engine" }));
    expect(await screen.findByRole("dialog", { name: "Choose a model" })).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("group", { name: "Before the first Cursor run" })).toBeNull());
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Choose a model" })).toBeNull());
    expect(await screen.findByRole("group", { name: "Before the first Cursor run" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(field()).toHaveValue("Run the tests.");
  });
});

const refusedCard = () => screen.getByRole("group", { name: "Cursor can’t start in Supervised mode" });
const supervisedThread = { ...thread, lifecycle: { permission_mode: "supervised" } } as unknown as Thread;
const refuseFirst = (mode = "supervised") => {
  const saved = { ...thread, lifecycle: { permission_mode: mode } } as unknown as Thread;
  return render(<Composer thread={saved} engine={cursor} showContext />);
};

describe("a refused mode", () => {
  it("draws the card with the frame's words and both actions, dims Send, keeps the message and shows the saved mode dimmed", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", "<img src=x onerror=alert(1)> Not here."));
    render(<Composer thread={supervisedThread} engine={cursor} showContext />);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const card = await screen.findByRole("group", { name: "Cursor can’t start in Supervised mode" });
    expect(card).toHaveTextContent("Cursor does not ask before acting, so it can’t run where approvals are required.");
    expect(card).toHaveTextContent("Running without approvals applies to this thread only. Orbyn does not remember it.");
    expect(card.querySelector("img")).toBeNull();
    expect(screen.getByRole("button", { name: "Run this thread without approvals" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Choose another engine" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(field()).toHaveValue("Run the tests.");
    expect(screen.getByRole("button", { name: "Permissions: Supervised" })).toBeDisabled();
  });

  it("names Auto edits the way the frame does", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", ""));
    refuseFirst("auto_edits");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByRole("group", { name: "Cursor can’t start in Auto edits mode" })).toBeVisible();
  });

  it("Run this thread without approvals is one write for one turn, even pressed twice, and never touches the saved mode", async () => {
    const start = vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", "x"));
    const patch = vi.spyOn(api, "patchThread");
    const mock = createCursorThreadMock({ delayMs: 20 });
    const send = vi.spyOn(mock, "sendWithoutApprovals");
    setCursorThreadApi(mock);
    render(<Composer thread={supervisedThread} engine={cursor} showContext />);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const go = await screen.findByRole("button", { name: "Run this thread without approvals" });
    fireEvent.click(go);
    fireEvent.click(go);
    await waitFor(() => expect(field()).toHaveValue(""));
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith("t1", { prompt: "Run the tests.", attachment_ids: [] });
    expect(mock.writes.override).toEqual(["t1"]);
    expect(start).toHaveBeenCalledTimes(1);
    expect(patch).not.toHaveBeenCalled();
    expect(screen.queryByRole("group", { name: "Cursor can’t start in Supervised mode" })).toBeNull();
  });

  it("keeps the card and the message when the one-turn write fails, and says so", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", "x"));
    setCursorThreadApi(null);
    render(<Composer thread={supervisedThread} engine={cursor} showContext />);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.click(await screen.findByRole("button", { name: "Run this thread without approvals" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't start the run.");
    expect(refusedCard()).toBeVisible();
    expect(screen.getByRole("button", { name: "Run this thread without approvals" })).toBeEnabled();
    expect(field()).toHaveValue("Run the tests.");
  });

  it("Choose another engine opens the picker, the card steps aside and returns if it closes with Cursor still chosen", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", "x"));
    const user = userEvent.setup();
    render(<Composer thread={supervisedThread} engine={cursor} showContext />);
    await user.click(screen.getByRole("button", { name: "Send" }));
    await user.click(await screen.findByRole("button", { name: "Choose another engine" }));
    expect(await screen.findByRole("dialog", { name: "Choose a model" })).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("group", { name: "Cursor can’t start in Supervised mode" })).toBeNull());
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("group", { name: "Cursor can’t start in Supervised mode" })).toBeVisible();
    expect(field()).toHaveValue("Run the tests.");
  });

  it("when the one-turn run also needs the notice, the notice card comes first and the refusal card returns after Continue", async () => {
    const start = vi.spyOn(api, "startRun").mockRejectedValue(refusal("engine_mode_refused", "x"));
    const mock = createCursorThreadMock();
    vi.spyOn(mock, "sendWithoutApprovals").mockRejectedValueOnce(refusal("engine_notice_required"));
    setCursorThreadApi(mock);
    render(<Composer thread={supervisedThread} engine={cursor} showContext />);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.click(await screen.findByRole("button", { name: "Run this thread without approvals" }));
    expect(await screen.findByRole("group", { name: "Before the first Cursor run" })).toBeVisible();
    expect(screen.queryByRole("group", { name: "Cursor can’t start in Supervised mode" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("group", { name: "Cursor can’t start in Supervised mode" })).toBeVisible();
    expect(mock.writes.acknowledge).toEqual([{ project_id: "p", engine: "cursor" }]);
    expect(start).toHaveBeenCalledTimes(2);
    expect(field()).toHaveValue("Run the tests.");
  });

  it("another 409 or any other failure keeps the composer's own error line", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(new ApiFailure("thread_busy", "busy", 409));
    setup();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A run started first.");
    expect(screen.queryByRole("group", { name: /can’t start/ })).toBeNull();
  });
});

describe("the one mode", () => {
  it("reads No approvals (Cursor), cannot be opened, and says why", async () => {
    setup();
    const pick = screen.getByRole("button", { name: "Permissions: No approvals (Cursor)" });
    expect(pick).toBeDisabled();
    expect(pick).toHaveAttribute("title", "Cursor does not ask before acting, so this thread has one mode.");
    await userEvent.click(pick);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("an engine that falls back instead", () => {
  it("shows no dimmed rows and no notice", async () => {
    const claude = { id: "claude", name: "Claude Code", installed: true, capabilities: { managed: true, permission_modes: ["supervised", "auto"], skill_delivery: [] } } as unknown as EngineStatus;
    useWorkspace.setState({ engines: { data: [claude], error: null, loading: false } });
    render(<Composer thread={{ ...thread, engine: "claude", model: null, lifecycle: { permission_mode: "supervised" } } as unknown as Thread} engine={claude} showContext />);
    await userEvent.click(screen.getByRole("button", { name: "Permissions: Supervised" }));
    expect(screen.queryByRole("menuitemradio", { name: "Auto-accept edits" })).toBeNull();
    await act(async () => undefined);
    expect(document.querySelector(".access-refused")).toBeNull();
  });
});
