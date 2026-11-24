import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure } from "../../api/client";
import type { EngineStatus, Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { MessageActions, ThreadScope } from "./MessageActions";
const thread: Thread = { id: "t", project_id: "p", title: "Thread", engine: "codex", model: null,
  cwd: "/fixture", worktree: null, skill_profile: null, scratch: false, active_run: null,
  last_run_state: null, created_at: "", updated_at: "" };
beforeEach(() => useWorkspace.getState().putThread(thread));
afterEach(() => vi.restoreAllMocks());
it("edits a user message in the same thread without offering another fork action", async () => {
  const user = userEvent.setup();
  const rewind = vi.spyOn(api, "rewindThread").mockResolvedValue({ native: false, mode: "handoff" });
  vi.spyOn(api, "thread").mockResolvedValue(thread);
  const fork = vi.spyOn(api, "forkBefore");
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  expect(screen.queryByRole("button", { name: "Edit in a new thread" })).toBeNull();
  expect(screen.getByRole("button", { name: "Copy message" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Edit from here" }));
  await user.click(screen.getByRole("button", { name: "Rewind" }));
  await waitFor(() => expect(rewind).toHaveBeenCalledExactlyOnceWith("t", "r", false));
  expect(useUi.getState().drafts.t).toBe("Original prompt");
  expect(fork).not.toHaveBeenCalled();
});
it("says what a rewind hides and who goes back", async () => {
  const user = userEvent.setup();
  useWorkspace.setState({
    engines: { data: [{ id: "codex", name: "Codex" } as unknown as EngineStatus], error: null, loading: false },
  });
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  await user.click(screen.getByRole("button", { name: "Edit from here" }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Edit from here?" })).toBeInTheDocument();
  expect(within(dialog).getByText("The conversation continues from before this message. Messages from here on are hidden; the log keeps them.")).toBeInTheDocument();
  expect(within(dialog).getByText("Codex goes back in its own session when it can. When it can't, it carries on in a new session and is given the earlier conversation as text.")).toBeInTheDocument();
  expect(within(dialog).getByRole("checkbox", { name: "Also restore files to this point" })).not.toBeChecked();
});
it("keeps assistant forks and whole-checkpoint restore", async () => {
  const user = userEvent.setup();
  const fork = vi.spyOn(api, "forkThread").mockResolvedValue({ ...thread, id: "child" });
  const restore = vi.spyOn(api, "restoreWorkspace").mockResolvedValue(undefined);
  render(<ThreadScope value="t"><MessageActions text="Answer" runId="r" role="assistant" /></ThreadScope>);
  await user.click(screen.getByRole("button", { name: "Fork from this response" }));
  await waitFor(() => expect(useWorkspace.getState().threads.child).toBeDefined());
  expect(fork).toHaveBeenCalledExactlyOnceWith("t", "r");
  await user.click(screen.getByRole("button", { name: "Restore files to before this response" }));
  await user.click(screen.getByRole("button", { name: "Restore files" }));
  await waitFor(() => expect(restore).toHaveBeenCalledExactlyOnceWith("t", "r", "start"));
});
it("holds the dialog while a rewind is in flight", async () => {
  useUi.setState({ drafts: {} });
  let finish!: (value: Awaited<ReturnType<typeof api.rewindThread>>) => void;
  const pending = new Promise<Awaited<ReturnType<typeof api.rewindThread>>>((resolve) => { finish = resolve; });
  const rewind = vi.spyOn(api, "rewindThread").mockReturnValue(pending);
  vi.spyOn(api, "thread").mockResolvedValue(thread);
  const refresh = vi.spyOn(useWorkspace.getState(), "refreshThread");
  const user = userEvent.setup();
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  await user.click(screen.getByRole("button", { name: "Edit from here" }));
  const dialog = screen.getByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "Rewind" }));
  expect(rewind).toHaveBeenCalledExactlyOnceWith("t", "r", false);
  const rewinding = within(dialog).getByRole("button", { name: "Rewind" });
  expect(rewinding).toBeDisabled();
  expect(rewinding).toHaveAttribute("aria-busy", "true");
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
  await user.keyboard("{Escape}");
  // jsdom does not turn the key press into the dialog's cancel event, so send it too.
  fireEvent(dialog, new Event("cancel", { cancelable: true }));
  expect(screen.getByRole("dialog")).toBe(dialog);
  expect(useUi.getState().drafts.t).toBeUndefined();
  expect(refresh).not.toHaveBeenCalled();
  await act(async () => { finish({ native: false, mode: "handoff" }); });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(useUi.getState().drafts.t).toBe("Original prompt");
  expect(refresh).toHaveBeenCalledExactlyOnceWith("t");
});
it("keeps the dialog open with the service's sentence when a rewind fails", async () => {
  useUi.setState({ drafts: {} });
  const sentence = "1 other thread(s) work in this folder, so restoring could lose their work.";
  const rewind = vi.spyOn(api, "rewindThread").mockRejectedValue(new ApiFailure("shared_directory", sentence, 409));
  const user = userEvent.setup();
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  await user.click(screen.getByRole("button", { name: "Edit from here" }));
  await user.click(screen.getByRole("checkbox", { name: "Also restore files to this point" }));
  await user.click(screen.getByRole("button", { name: "Rewind" }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe(sentence);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("button", { name: "Rewind" })).toBeEnabled();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  expect(useUi.getState().drafts.t).toBeUndefined();
  expect(rewind).toHaveBeenCalledExactlyOnceWith("t", "r", true);
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("shows the service's sentence when the thread already has an active run", async () => {
  useUi.setState({ drafts: {} });
  const sentence = "this thread already has an active run";
  const rewind = vi.spyOn(api, "rewindThread").mockRejectedValue(new ApiFailure("thread_busy", sentence, 409));
  const user = userEvent.setup();
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  await user.click(screen.getByRole("button", { name: "Edit from here" }));
  await user.click(screen.getByRole("button", { name: "Rewind" }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe(sentence);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("button", { name: "Rewind" })).toBeEnabled();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  expect(useUi.getState().drafts.t).toBeUndefined();
  expect(rewind).toHaveBeenCalledExactlyOnceWith("t", "r", false);
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("turns Edit from here off while a turn runs, and keeps Copy", async () => {
  useWorkspace.getState().putThread({
    ...thread,
    active_run: { id: "run-live", thread_id: "t", state: "running", error: null, started_at: "", ended_at: null },
  });
  const user = userEvent.setup();
  render(<ThreadScope value="t"><MessageActions text="Original prompt" runId="r" role="user" /></ThreadScope>);
  const edit = screen.getByRole("button", { name: "Edit from here" });
  expect(edit).toBeDisabled();
  expect(edit).toHaveAccessibleName("Edit from here");
  // The icon's title is only its label; a run reason would replace or join it.
  expect(edit).toHaveAttribute("title", "Edit from here");
  expect(edit).not.toHaveAttribute("aria-description");
  expect(edit).not.toHaveAttribute("aria-describedby");
  expect(screen.getByRole("button", { name: "Copy message" })).toBeEnabled();
  await user.click(edit);
  expect(screen.queryByRole("dialog")).toBeNull();
});
