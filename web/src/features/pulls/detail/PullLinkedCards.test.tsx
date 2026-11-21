import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { Thread } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { breakdownBars, ChangeBreakdownCard, LinkedThreadsCard } from "./PullLinkedCards";

const counts = { files: 4n, added: 1n, modified: 2n, deleted: 0n, renamed: 1n, additions: 24n, deletions: 9n };
const thread = (id: string, title: string, over: Partial<Thread> = {}): Thread =>
  ({
    id, project_id: "p1", title, engine: "claude", model: null, cwd: "/x", worktree: { branch: "fix/login" }, skill_profile: null,
    active_run: null, last_run_state: "completed", created_at: "2026-11-20T12:00:00Z", updated_at: new Date(Date.now() - 2 * 3_600_000).toISOString(), scratch: false, ...over,
  }) as unknown as Thread;

beforeEach(() => {
  window.location.hash = "";
  useWorkspace.setState({ engines: { status: "ready", data: [{ id: "claude", name: "Claude Code" }], error: null } } as never);
});
afterEach(() => vi.restoreAllMocks());

describe("breakdownBars", () => {
  it("shares the bar between the kinds that have files, and draws none for a pull request with none", () => {
    expect(breakdownBars(counts).map((bar) => [bar.kind, bar.share])).toEqual([["added", 25], ["modified", 50], ["renamed", 25]]);
    expect(breakdownBars({ ...counts, files: 0n, added: 0n, modified: 0n, renamed: 0n })).toEqual([]);
  });
});

describe("Change breakdown card", () => {
  it("counts files by kind and the lines, in words as well as in the bar", async () => {
    vi.spyOn(pullsApi, "breakdown").mockResolvedValue(counts);
    render(<ChangeBreakdownCard projectId="p1" number={142} />);
    expect(await screen.findByText("4 files")).toBeInTheDocument();
    for (const [name, count] of [["Added", "1"], ["Modified", "2"], ["Deleted", "0"], ["Renamed", "1"]]) {
      expect(screen.getByText(name!).parentElement).toHaveTextContent(`${count}${name}`);
    }
    expect(screen.getByText(/\+24/)).toBeInTheDocument();
    expect(screen.getByText(/−9/)).toBeInTheDocument();
  });

  it("fails on its own with a Retry that asks again", async () => {
    const spy = vi.spyOn(pullsApi, "breakdown").mockRejectedValueOnce(new ApiFailure("unexpected", "GitHub did not send the file list.", 502)).mockResolvedValue(counts);
    const user = userEvent.setup();
    render(<ChangeBreakdownCard projectId="p1" number={142} />);
    expect(await screen.findByText(/Can’t load the breakdown/)).toBeInTheDocument();
    expect(screen.getByText(/GitHub did not send the file list/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("4 files")).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});

describe("Linked threads card", () => {
  it("lists each thread with its engine, branch and state, and a row opens the thread", async () => {
    vi.spyOn(pullsApi, "linkedThreads").mockResolvedValue([
      thread("t1", "Fix login redirect", { active_run: { state: "running" } as Thread["active_run"] }),
      thread("t2", "Redirect tests"),
      thread("t3", "Try cookie-based redirect", { last_run_state: "failed" }),
    ]);
    const user = userEvent.setup();
    render(<LinkedThreadsCard projectId="p1" number={142} />);
    expect(await screen.findByText("Linked threads · 3")).toBeInTheDocument();
    const rows = screen.getAllByRole("link");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText("Running")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("2h")).toBeInTheDocument();
    expect(within(rows[2]!).getByText("Failed")).toBeInTheDocument();
    expect(within(rows[0]!).getByText(/Claude Code/)).toBeInTheDocument();
    expect(within(rows[0]!).getByText("fix/login")).toBeInTheDocument();
    await user.click(rows[1]!);
    expect(window.location.hash).toBe("#/threads/t2");
  });

  it("draws a hostile title as text", async () => {
    vi.spyOn(pullsApi, "linkedThreads").mockResolvedValue([thread("t1", "<script>alert(1)</script>")]);
    const { container } = render(<LinkedThreadsCard projectId="p1" number={1} />);
    expect(await screen.findByText("<script>alert(1)</script>")).toBeInTheDocument();
    expect(container.querySelector("script")).toBeNull();
  });

  it("says so when no thread is linked", async () => {
    vi.spyOn(pullsApi, "linkedThreads").mockResolvedValue([]);
    render(<LinkedThreadsCard projectId="p1" number={142} />);
    expect(await screen.findByText("No thread is linked to this pull request.")).toBeInTheDocument();
    expect(screen.getByText("Linked threads")).toBeInTheDocument();
  });

  it("says nothing is changed when it cannot load, and Retry asks again", async () => {
    const spy = vi.spyOn(pullsApi, "linkedThreads").mockRejectedValueOnce(new ApiFailure("unexpected", "x", 500)).mockResolvedValue([thread("t1", "One")]);
    const user = userEvent.setup();
    render(<LinkedThreadsCard projectId="p1" number={142} />);
    expect(await screen.findByText("Can’t load the linked threads. Nothing is changed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.getByRole("link")).toBeInTheDocument());
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("keeps its title and shows quiet bars while it loads, after the usual wait", async () => {
    vi.spyOn(pullsApi, "linkedThreads").mockReturnValue(new Promise(() => undefined));
    render(<LinkedThreadsCard projectId="p1" number={142} />);
    expect(screen.getByText("Linked threads")).toBeInTheDocument();
    expect(await screen.findByLabelText("Loading linked threads")).toHaveAttribute("aria-busy", "true");
  });
});
