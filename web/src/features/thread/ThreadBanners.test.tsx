import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure, api } from "../../api/client";
import { useUi } from "../../product/state/ui";
import type { AgUiEvent, Thread, WireThreadEvent } from "../../api/types";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useWorkspace } from "../../product/state/workspace";
import { applyEvents, emptyLog } from "../../product/timeline/projection";
import { LimitBanner, ThreadErrorBanner } from "./ThreadBanners";

const wire = (seq: number, event: AgUiEvent): WireThreadEvent => ({
  seq,
  thread_id: "t",
  run_id: "r",
  ts: "2026-11-01T00:00:00Z",
  event,
});

const prompt = [
  wire(1, { type: "TEXT_MESSAGE_START", messageId: "u1", role: "user" }),
  wire(2, { type: "TEXT_MESSAGE_CONTENT", messageId: "u1", delta: "Find out why login sends people back to /home." }),
  wire(3, { type: "TEXT_MESSAGE_END", messageId: "u1" }),
];

const failedLog = (message: string) =>
  useThreadLogs.setState({
    logs: {
      t: applyEvents("t", emptyLog, [
        ...prompt,
        wire(9, { type: "RUN_ERROR", message, code: "engine_failed" }),
      ]),
    },
  });

const reset = "2100-01-01T00:00:00Z";
const thread: Thread = { id: "t", project_id: "p", title: "Limited", engine: "codex", model: null,
  cwd: "/fixture", worktree: null, skill_profile: null, scratch: false, active_run: null, last_run_state: "failed", created_at: "", updated_at: "",
  lifecycle: { pinned_at: null, archived_at: null, settled_at: null, snoozed_until: null, unread: false, auto_settle: null,
    position: 0, parent_thread_id: null, relation: null, permission_mode: "supervised", plan_mode: false,
    limit: { message: "Usage limit reached", stopped_at: "2026-11-01T00:00:00Z", resets_at: reset, resume_at: null } } };
afterEach(() => { vi.restoreAllMocks(); useUi.setState({ toasts: [] }); });

it("schedules and cancels auto-resume through the existing thread patch", async () => {
  const user = userEvent.setup(); const patch = vi.spyOn(api, "patchThread").mockResolvedValue(thread);
  const view = render(<LimitBanner thread={thread} />);
  await user.click(screen.getByRole("button", { name: "Resume at reset" }));
  expect(patch).toHaveBeenCalledExactlyOnceWith("t", { resume_at_reset: true });
  view.rerender(<LimitBanner thread={{ ...thread, lifecycle: { ...thread.lifecycle!, limit: { ...thread.lifecycle!.limit!, resume_at: reset } } }} />);
  await user.click(screen.getByRole("button", { name: "Cancel auto-resume" }));
  expect(patch).toHaveBeenLastCalledWith("t", { resume_at_reset: false });
});

it("offers Wake now for a matching reset snooze without starting a run", async () => {
  const user = userEvent.setup(); const patch = vi.spyOn(api, "patchThread").mockResolvedValue(thread);
  const run = vi.spyOn(api, "startRun");
  render(<LimitBanner thread={{ ...thread, lifecycle: { ...thread.lifecycle!, snoozed_until: "2100-01-01T01:00:00+01:00" } }} />);
  expect(screen.queryByRole("button", { name: "Snooze until reset" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Wake now" }));
  await waitFor(() => expect(patch).toHaveBeenCalledExactlyOnceWith("t", { wake: true }));
  expect(run).not.toHaveBeenCalled();
});

it("keeps the reset snooze action for a thread not snoozed until that instant", async () => {
  const user = userEvent.setup(); const patch = vi.spyOn(api, "patchThread").mockResolvedValue(thread);
  render(<LimitBanner thread={thread} />);
  await user.click(screen.getByRole("button", { name: "Snooze until reset" }));
  expect(patch).toHaveBeenCalledExactlyOnceWith("t", { snoozed_until: reset });
});

it("sends one write for a limit action however fast it is pressed, keeps focus on it and takes the next press after the answer", async () => {
  const user = userEvent.setup();
  let answer: (value: Thread) => void = () => undefined;
  const patch = vi.spyOn(api, "patchThread").mockImplementation(() => new Promise<Thread>((resolve) => { answer = resolve; }));
  render(<LimitBanner thread={thread} />);
  const schedule = screen.getByRole("button", { name: "Resume at reset" });
  await user.click(schedule);
  await user.click(schedule);
  expect(schedule).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Snooze until reset" }));
  expect(patch).toHaveBeenCalledExactlyOnceWith("t", { resume_at_reset: true });
  expect(schedule).toHaveAttribute("aria-disabled", "true");
  answer(thread);
  await waitFor(() => expect(schedule).not.toHaveAttribute("aria-disabled"));
  await user.click(screen.getByRole("button", { name: "Snooze until reset" }));
  expect(patch).toHaveBeenCalledTimes(2);
  expect(patch).toHaveBeenLastCalledWith("t", { snoozed_until: reset });
});

it("sends Resume once however fast it is pressed, and says why when the service refuses", async () => {
  const user = userEvent.setup();
  let refuse: (failure: ApiFailure) => void = () => undefined;
  const run = vi.spyOn(api, "startRun").mockImplementation(() => new Promise((_, reject) => { refuse = reject; }));
  render(<LimitBanner thread={thread} />);
  const resume = screen.getByRole("button", { name: "Resume" });
  await user.dblClick(resume);
  refuse(new ApiFailure("unavailable", "The service did not answer.", 503));
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Couldn't do that. The service did not answer."]));
  expect(run).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Resume at reset" })).toBeVisible();
});

const failed: Thread = { ...thread, lifecycle: { ...thread.lifecycle!, limit: undefined } };

describe("ThreadErrorBanner", () => {
  it("drops Retry from a no-reply failure; Resend on the failed run replaces it", () => {
    failedLog("No reply: the engine finished without answering. Send your message again.");
    render(<ThreadErrorBanner thread={failed} />);
    expect(screen.getByText("No reply: the engine finished without answering. Send your message again.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(screen.getByRole("button", { name: "Dismiss error" })).toBeTruthy();
  });

  it("drops Retry from a lost-conversation failure", () => {
    failedLog("The engine lost this conversation.");
    render(<ThreadErrorBanner thread={failed} />);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("leaves the host failures to the failed run's own actions instead of a second Retry", () => {
    for (const code of ["engine_host_stopped", "engine_unavailable"]) {
      useThreadLogs.setState({
        logs: { t: applyEvents("t", emptyLog, [...prompt, wire(9, { type: "RUN_ERROR", message: "The engine host stopped.", code })]) },
      });
      useWorkspace.setState({ engines: { data: [{ id: "claude", name: "Claude Code", installed: true, capabilities: {} }] as never, error: null, loading: false } });
      const view = render(<ThreadErrorBanner thread={{ ...failed, engine: "claude" }} />);
      expect(screen.queryByText("The engine host stopped.")).toBeNull();
      expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
      view.unmount();
    }
  });

  it("keeps Retry for another failure and sends the prompt again", async () => {
    const user = userEvent.setup();
    const run = vi.spyOn(api, "startRun").mockResolvedValue({} as never);
    failedLog("Codex exited unexpectedly with status 137.");
    render(<ThreadErrorBanner thread={failed} />);
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(run).toHaveBeenCalledExactlyOnceWith("t", "Find out why login sends people back to /home.");
  });
});
