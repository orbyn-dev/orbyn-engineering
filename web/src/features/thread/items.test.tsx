import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { AgUiEvent, Thread, WireThreadEvent } from "../../api/types";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useWorkspace } from "../../product/state/workspace";
import { applyEvents, emptyLog } from "../../product/timeline/projection";
import { ThreadItemView } from "./items";
import { TimelineThread } from "./timelineThread";

const NO_REPLY = "No reply: the engine finished without answering. Send your message again.";
const LOST = "The engine lost this conversation.";

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

const failedLog = (message: string) => {
  const log = applyEvents("t", emptyLog, [
    ...prompt,
    wire(9, { type: "RUN_ERROR", message, code: "engine_failed" }),
  ]);
  useThreadLogs.setState({ logs: { t: log } });
  const failure = log.items.findLast((item) => item.kind === "notice");
  if (!failure) throw new Error("no failure item in the test log");
  return failure;
};

const view = (item: ReturnType<typeof failedLog>) =>
  render(
    <TimelineThread value="t">
      <ThreadItemView item={item} live={false} />
    </TimelineThread>,
  );

afterEach(() => vi.restoreAllMocks());

it("offers Resend on a no-reply failure and sends that run's prompt again", async () => {
  const user = userEvent.setup();
  const run = vi.spyOn(api, "startRun").mockResolvedValue({} as never);
  view(failedLog(NO_REPLY));
  await user.click(screen.getByRole("button", { name: "Resend" }));
  expect(run).toHaveBeenCalledExactlyOnceWith("t", "Find out why login sends people back to /home.");
});

it("offers Resend on a lost-conversation failure", () => {
  view(failedLog(LOST));
  expect(screen.getByRole("button", { name: "Resend" })).toBeTruthy();
});

it("shows no Resend and keeps the code pill for another failure", () => {
  view(failedLog("Codex exited unexpectedly with status 137."));
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
  expect(screen.getByText("engine_failed")).toBeTruthy();
});

it("shows no code pill on a no-reply failure", () => {
  view(failedLog(NO_REPLY));
  expect(screen.queryByText("engine_failed")).toBeNull();
});

it("keeps Resend disabled while it sends", async () => {
  const user = userEvent.setup();
  vi.spyOn(api, "startRun").mockReturnValue(new Promise(() => {}) as never);
  view(failedLog(NO_REPLY));
  const resend = screen.getByRole("button", { name: "Resend" }) as HTMLButtonElement;
  expect(resend.disabled).toBe(false);
  await user.click(resend);
  expect(screen.getByRole("button", { name: "Resend" }).getAttribute("disabled")).not.toBeNull();
});

it("keeps Resend disabled while the thread has a run going", () => {
  const thread: Thread = {
    id: "t", project_id: "p", title: "Failed", engine: "codex", model: null, cwd: "/fixture",
    worktree: null, skill_profile: null, scratch: false, active_run: {
      id: "r2", thread_id: "t", state: "running", error: null, started_at: "", ended_at: null,
    }, last_run_state: "failed", created_at: "", updated_at: "",
  };
  useWorkspace.setState({ threads: { t: thread } });
  view(failedLog(NO_REPLY));
  expect(screen.getByRole("button", { name: "Resend" }).getAttribute("disabled")).not.toBeNull();
});
