import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, api } from "../../api/client";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../state/workspace";
import * as actions from "../workspace/threadActions";
import { ArchivedBanner } from "./ThreadStates";
import { emptyLog } from "./projection";
import { familyOf, ThreadHeaderMenu } from "./ThreadHeaderMenu";
import { useThreadLogs } from "./threadLogs";

vi.mock("../workspace/threadActions", async (original) => ({
  ...(await original<typeof import("../workspace/threadActions")>()),
  archiveThreads: vi.fn(),
  unarchiveThread: vi.fn(),
  copyText: vi.fn(),
  deleteThreadNow: vi.fn(),
}));

const base: Thread = { id: "t", project_id: "p", title: "Fix login", engine: "codex", model: null,
  cwd: "/work/app", worktree: null, skill_profile: null, scratch: false, active_run: null, last_run_state: "completed", created_at: "", updated_at: "",
  lifecycle: { pinned_at: null, archived_at: null, settled_at: null, snoozed_until: null, unread: false, auto_settle: null,
    position: 0, parent_thread_id: null, relation: null, permission_mode: "supervised", plan_mode: false } };
const child = (id: string, running: boolean): Thread => ({
  ...base, id, title: id, active_run: running ? ({ id: `r-${id}`, thread_id: id, state: "running", error: null, started_at: "", ended_at: null } as unknown as Thread["active_run"]) : null,
  lifecycle: { ...base.lifecycle!, parent_thread_id: "t", relation: "side" },
});

const open = async (thread: Thread) => {
  const user = userEvent.setup();
  render(<ThreadHeaderMenu thread={thread} />);
  await user.click(screen.getByRole("button", { name: "Thread menu" }));
  return { user, menu: await screen.findByRole("menu", { name: "Thread menu" }) };
};

beforeEach(() => {
  vi.mocked(actions.archiveThreads).mockClear();
  vi.mocked(actions.unarchiveThread).mockClear();
  vi.mocked(actions.copyText).mockClear();
  vi.mocked(actions.deleteThreadNow).mockReset();
  useWorkspace.setState({ threads: { t: base } });
});

describe("the thread header menu", () => {
  it("copies the folder path", async () => {
    const { user, menu } = await open(base);
    await user.click(within(menu).getByRole("menuitem", { name: "Copy folder path" }));
    expect(actions.copyText).toHaveBeenCalledWith("/work/app", "Folder path copied");
  });

  it("archives a live thread", async () => {
    const { user, menu } = await open(base);
    await user.click(within(menu).getByRole("menuitem", { name: "Archive" }));
    expect(actions.archiveThreads).toHaveBeenCalledWith([base]);
  });

  it("offers Unarchive, not Archive, on an archived thread", async () => {
    const archived = { ...base, lifecycle: { ...base.lifecycle!, archived_at: "2026-11-03T00:00:00Z" } };
    const { user, menu } = await open(archived);
    expect(within(menu).queryByRole("menuitem", { name: "Archive" })).toBeNull();
    await user.click(within(menu).getByRole("menuitem", { name: "Unarchive" }));
    expect(actions.unarchiveThread).toHaveBeenCalledWith(archived);
  });

  it("cannot unarchive while the project folder is removed", async () => {
    const archived = { ...base, project_removed: true, lifecycle: { ...base.lifecycle!, archived_at: "2026-11-03T00:00:00Z" } };
    const { menu } = await open(archived);
    expect(within(menu).getByRole("menuitem", { name: "Unarchive" })).toBeDisabled();
  });

  it("will not delete while the thread or any side chat or child runs, and says why", async () => {
    useWorkspace.setState({ threads: { t: base, s1: child("s1", true) } });
    const { menu } = await open(base);
    const item = within(menu).getByRole("menuitem", { name: "Delete thread" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", expect.stringMatching(/side chats/));
    expect(familyOf(base, { t: base, s1: child("s1", false), g: { ...child("g", true), lifecycle: { ...base.lifecycle!, parent_thread_id: "s1" } } }).map((t) => t.id)).toEqual(["t", "s1", "g"]);
  });

  it("asks first, says side chats and children go too, and keeps the dialog open with the error when the service refuses", async () => {
    vi.mocked(actions.deleteThreadNow).mockRejectedValue(new ApiFailure("thread_busy", "A thread in this family is running.", 409));
    const { user, menu } = await open(base);
    await user.click(within(menu).getByRole("menuitem", { name: "Delete thread" }));
    const dialog = await screen.findByRole("dialog", { name: /Delete “Fix login”/ });
    expect(dialog).toHaveTextContent(/side chats/);
    await user.click(within(dialog).getByRole("button", { name: "Delete thread" }));
    await waitFor(() => expect(dialog).toHaveTextContent("A thread in this family is running."));
    expect(screen.getByRole("dialog", { name: /Delete “Fix login”/ })).toBeInTheDocument();
  });

  it("closes the dialog only after the delete succeeds", async () => {
    vi.mocked(actions.deleteThreadNow).mockResolvedValue(undefined);
    const { user, menu } = await open(base);
    await user.click(within(menu).getByRole("menuitem", { name: "Delete thread" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete thread" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(actions.deleteThreadNow).toHaveBeenCalledWith(base);
  });
});

describe("the archived banner", () => {
  it("forks from the end of the thread", async () => {
    useThreadLogs.setState((state) => ({
      logs: { ...state.logs, t: { ...emptyLog, items: [{ kind: "message", key: "m", seq: 1, runId: "run-9", ts: "", role: "user", text: "hi", done: true }] } },
    }));
    const fork = vi.spyOn(api, "forkThread").mockResolvedValue({ ...base, id: "child" });
    render(<ArchivedBanner thread={base} />);
    await userEvent.click(screen.getByRole("button", { name: "Fork to new thread" }));
    expect(fork).toHaveBeenCalledWith("t", "run-9");
  });

  it("keeps its buttons out of the live region", () => {
    render(<ArchivedBanner thread={base} />);
    expect(screen.getByRole("status")).not.toContainElement(screen.getByRole("button", { name: "Unarchive" }));
  });
});
