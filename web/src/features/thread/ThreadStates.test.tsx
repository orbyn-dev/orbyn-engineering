import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Thread } from "../../api/types";
import { LimitBanner } from "./ThreadBanners";
import { TimelineRow } from "./items";
import type { ThreadItem } from "./projection";
import { emptyLog } from "./projection";
import { ArchivedBanner, CompactingStep, useCompacting } from "./ThreadStates";
import { useThreadLogs } from "./threadLogs";

const thread: Thread = { id: "t", project_id: "p", title: "T", engine: "codex", model: null,
  cwd: "/fixture", worktree: null, skill_profile: null, scratch: false, active_run: null, last_run_state: "completed", created_at: "", updated_at: "",
  lifecycle: { pinned_at: null, archived_at: null, settled_at: null, snoozed_until: null, unread: false, auto_settle: null,
    position: 0, parent_thread_id: null, relation: null, permission_mode: "supervised", plan_mode: false } };
const running = { id: "r1", thread_id: "t", state: "running", error: null, started_at: "", ended_at: null } as unknown as Thread["active_run"];
const user = (text: string): ThreadItem => ({ kind: "message", key: "u", seq: 1, runId: "r1", ts: "", role: "user", text, done: true });
const load = (items: ThreadItem[]) => useThreadLogs.setState((state) => ({ logs: { ...state.logs, t: { ...emptyLog, items } } }));

beforeEach(() => useThreadLogs.setState({ logs: {}, streams: {}, prompts: {} }));
afterEach(() => vi.useRealTimers());

it("marks where a run was stopped, with the time, and keeps it in view", () => {
  const notice: ThreadItem = { kind: "notice", key: "n", seq: 2, runId: "r1", ts: "2026-11-08T14:02:00Z", tone: "neutral", text: "Stopped by you", code: "run_stopped" };
  render(<TimelineRow entry={notice} live={false} tools={[]} />);
  expect(screen.getByText(/Stopped by you/)).toHaveTextContent(/^Stopped by you · /);
});

it("shows the compact step while a compact run is going, and not for any other run", () => {
  load([user("/compact")]);
  const view = render(<CompactingStep thread={{ ...thread, active_run: running }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Compacting");
  view.unmount();
  load([user("hello")]);
  render(<CompactingStep thread={{ ...thread, active_run: running }} />);
  expect(screen.queryByRole("status")).toBeNull();
});

it("tells the composer to wait only while compacting", () => {
  load([user("/compact")]);
  const seen: boolean[] = [];
  function Probe({ value }: { value: Thread }) {
    seen.push(useCompacting(value));
    return null;
  }
  render(<Probe value={{ ...thread, active_run: running }} />);
  render(<Probe value={thread} />);
  expect(seen).toEqual([true, false]);
});

it("says an archived thread is read-only, with Unarchive, and not unarchivable until its folder is back", () => {
  const view = render(<ArchivedBanner thread={thread} />);
  expect(screen.getByText("This thread is archived and read-only.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Unarchive" })).toBeEnabled();
  view.unmount();
  render(<ArchivedBanner thread={{ ...thread, project_removed: true }} />);
  expect(screen.getByRole("button", { name: "Unarchive" })).toBeDisabled();
});

it("counts down the wait on a rate limit, with the time left", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-08T14:00:00Z"));
  const limited = { ...thread, lifecycle: { ...thread.lifecycle!, limit: { message: "Usage limit reached.", stopped_at: "2026-11-08T13:58:00Z", resets_at: "2026-11-08T14:02:14Z", resume_at: null } } };
  render(<LimitBanner thread={limited} />);
  expect(screen.getByText("Rate limited")).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Try again in 2m 14s");
});
