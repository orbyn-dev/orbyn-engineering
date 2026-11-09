import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Thread } from "../../api/types";
import { LimitBanner } from "./ThreadBanners";
import { TimelineRow } from "./items";
import type { ThreadItem } from "../../lib/timeline/projection";
import { emptyLog } from "../../lib/timeline/projection";
import { ArchivedBanner, CompactingStep, useCompacting, useThreadChip } from "./ThreadStates";
import { useThreadLogs } from "../../state/threadLogs";

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
  const notice: ThreadItem = { kind: "notice", key: "n", seq: 2, runId: "r1", ts: "2026-11-08T14:02:00Z", tone: "neutral", text: "Stopped", code: "run_stopped" };
  render(<TimelineRow entry={notice} live={false} tools={[]} />);
  expect(screen.getByText(/Stopped/)).toHaveTextContent(/^Stopped · /);
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

it("counts down a rate limit outside the live region, and ends the wait at zero", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-08T14:00:00Z"));
  const limited = { ...thread, lifecycle: { ...thread.lifecycle!, limit: { message: "Usage limit reached.", stopped_at: "2026-11-08T13:58:00Z", resets_at: "2026-11-08T14:02:14Z", resume_at: null } } };
  render(<LimitBanner thread={limited} />);
  const group = screen.getByRole("group", { name: "Limited" });
  expect(group).toHaveTextContent("Rate limited");
  expect(group).toHaveTextContent("Try again in 2m 14s");
  // The live region never holds the ticking text.
  expect(screen.getByRole("status")).not.toHaveTextContent(/\d+m \d+s/);
  expect(screen.getByRole("button", { name: "Resume at reset" })).toBeInTheDocument();
  act(() => vi.advanceTimersByTime(60_000));
  expect(group).toHaveTextContent("Try again in 1m 14s");
  act(() => vi.advanceTimersByTime(75_000));
  expect(group).not.toHaveTextContent("Try again in");
  expect(screen.getByRole("status")).toHaveTextContent("The wait is over.");
  expect(screen.queryByRole("button", { name: "Resume at reset" })).toBeNull();
  expect(screen.queryByRole("button", { name: /Snooze until reset/ })).toBeNull();
  expect(screen.getByRole("button", { name: "Resume" })).toBeInTheDocument();
});

function Chip({ value }: { value: Thread }) {
  return <>{useThreadChip(value)}</>;
}

it("says in the header when a thread is archived, compacting or waiting out a limit", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-08T14:00:00Z"));
  const archived = render(<Chip value={{ ...thread, lifecycle: { ...thread.lifecycle!, archived_at: "2026-11-03T09:00:00Z" } }} />);
  expect(archived.container).toHaveTextContent(/^Archived 3 Nov$/);
  archived.unmount();
  load([user("/compact")]);
  const compacting = render(<Chip value={{ ...thread, active_run: running }} />);
  expect(compacting.container).toHaveTextContent("Compacting");
  compacting.unmount();
  const limited = { ...thread, lifecycle: { ...thread.lifecycle!, limit: { message: "x", stopped_at: "2026-11-08T13:58:00Z", resets_at: "2026-11-08T14:02:14Z", resume_at: null } } };
  const waiting = render(<Chip value={limited} />);
  expect(waiting.container).toHaveTextContent("Waiting · 2m 14s");
  waiting.unmount();
  expect(render(<Chip value={thread} />).container).toBeEmptyDOMElement();
});
