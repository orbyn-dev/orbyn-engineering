import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../../api/pulls";
import { ApiFailure } from "../../api/client";
import type { PullCheck, PullRequestDetail } from "../../api/types";
import { useConnection } from "../../state/connection";
import { PullDetail } from "./PullDetail";

const pull = (over: Record<string, unknown> = {}): PullRequestDetail["pull"] =>
  ({
    number: 142,
    title: "Return to the page people came from",
    url: "https://github.com/o/r/pull/142",
    state: "open",
    head: "fix/login",
    base: "main",
    author: "riley",
    draft: false,
    updated_at: null,
    labels: [],
    review_state: null,
    checks_state: "pass",
    checks: { total: 4, passing: 4, failing: 0, running: 0, failing_names: [] },
    ...over,
  }) as PullRequestDetail["pull"];

const detail = (over: Record<string, unknown> = {}, mergeable = "mergeable"): PullRequestDetail =>
  ({
    pull: pull(over),
    body: "Login now returns people back.",
    checks: [],
    comments: [],
    mergeable,
    reviewers: ["sam"],
    head_sha: "abc",
    base_sha: "def",
    additions: 4,
    deletions: 3,
    changed_files: 2,
    maintainer_can_modify: null,
    auto_merge: false,
  }) as PullRequestDetail;

const checks: PullCheck[] = [
  { name: "lint", state: "pass", url: null, workflow: null, description: null },
  { name: "e2e (macOS)", state: "fail", url: "https://example.com/log", workflow: null, description: "login ignores unsafe next values" },
];

const show = async (response: PullRequestDetail | Promise<PullRequestDetail>) => {
  vi.mocked(pullsApi.detail).mockImplementation(() => Promise.resolve(response));
  render(<PullDetail layout="page" projectId="p" number={142} />);
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
};

beforeEach(() => {
  vi.useFakeTimers();
  useConnection.setState({ offline: false, serviceDown: false });
  vi.spyOn(pullsApi, "detail");
  vi.spyOn(pullsApi, "checks").mockResolvedValue(checks);
  vi.spyOn(pullsApi, "files").mockResolvedValue([
    { path: "auth/redirect.ts", previous_path: null, status: "modified", additions: 2, deletions: 1, patch: null, viewed: false },
  ]);
  vi.spyOn(pullsApi, "details").mockResolvedValue({ details: [{ project_id: "p", number: 142, comment_count: 3 }], paused: [] } as never);
  vi.spyOn(pullsApi, "forges").mockResolvedValue([{ id: "github", name: "GitHub", program: "gh", installed: true, version: null, signed_in: true, accounts: ["me"], viewer: "me", problem: null }]);
  vi.spyOn(pullsApi, "stack").mockResolvedValue({ layers: [], position: 0 } as never);
  vi.spyOn(pullsApi, "reviewerCandidates").mockResolvedValue([]);
  vi.spyOn(pullsApi, "labelCandidates").mockResolvedValue([]);
  vi.spyOn(pullsApi, "update").mockResolvedValue(undefined);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const merge = () => screen.getByRole("button", { name: /^Merge$/ });

describe("one pull request as a page", () => {
  it("names the failing check in a banner, links its log and blocks Merge", async () => {
    await show(detail({ checks_state: "fail", checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e (macOS)"] } }));
    const banner = screen.getByText("e2e (macOS) failed", { selector: "b" }).closest(".pulls-banner") as HTMLElement;
    expect(banner).toHaveTextContent("login ignores unsafe next values");
    expect(within(banner).getByRole("link", { name: "View log" })).toHaveAttribute("href", "https://example.com/log");
    expect(merge()).toBeDisabled();
    expect(screen.getByText("Merge is blocked by a failing check.")).toBeInTheDocument();
  });

  it("offers Ready for review on a draft, which marks it ready", async () => {
    await show(detail({ draft: true }));
    expect(screen.getByText("Draft. Nobody is asked to review it yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ready for review" }));
    expect(pullsApi.update).toHaveBeenCalledWith("p", 142, { draft: false });
  });

  it("counts files, checks and comments in the tab names", async () => {
    await show(detail());
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual(["Summary", "Guide", "Conversation · 3", "Files changed · 2", "Checks · 4"]);
  });

  it("chooses the merge method and options from the Merge method menu", async () => {
    await show(detail({ review_state: "approved" }));
    fireEvent.click(screen.getByRole("button", { name: /Squash and merge/ }));
    const menu = screen.getByRole("menu", { name: "Merge method" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Squash and merge",
      "Create a merge commit",
      "Rebase and merge",
      "Merge when checks pass",
      "Delete branch after merge",
    ]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Rebase and merge" }));
    expect(screen.getByRole("button", { name: /Rebase and merge/ })).toBeInTheDocument();
  });

  it("says it is offline, with Retry, and Merge waits for a connection", async () => {
    useConnection.setState({ offline: true });
    await show(detail());
    expect(screen.getByText(/You’re offline\. Showing what was loaded/)).toBeInTheDocument();
    expect(screen.getByText("Merge needs a connection.")).toBeInTheDocument();
    expect(merge()).toBeDisabled();
  });

  it("says the forge did not answer when the page cannot load, and Retry asks again", async () => {
    vi.mocked(pullsApi.detail).mockRejectedValue(new ApiFailure("bad_gateway", "no", 502));
    render(<PullDetail layout="page" projectId="p" number={142} />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByText("Can’t reach GitHub")).toBeInTheDocument();
    expect(screen.getByText("Pull request #142 did not load. Nothing was changed on GitHub or in your thread.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(pullsApi.detail).toHaveBeenCalledTimes(2);
  });

  it("shows nothing for 400 ms while it loads, then quiet cards", async () => {
    vi.mocked(pullsApi.detail).mockReturnValue(new Promise(() => {}));
    render(<PullDetail layout="page" projectId="p" number={142} />);
    expect(document.querySelectorAll(".pull-skel-card")).toHaveLength(0);
    act(() => void vi.advanceTimersByTime(400));
    expect(document.querySelectorAll(".pull-skel-card").length).toBeGreaterThan(0);
  });

  it("filters the changed files, and says what the pull request does change when none match", async () => {
    vi.spyOn(pullsApi, "revisions").mockResolvedValue([]);
    await show(detail());
    fireEvent.click(screen.getByRole("tab", { name: /^Files changed/ }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent("Viewed 0 of 1");
    fireEvent.change(screen.getByRole("textbox", { name: "Filter files" }), { target: { value: "migrations" } });
    expect(screen.getByText("No files match \u201cmigrations\u201d")).toBeInTheDocument();
    expect(screen.getByText("This pull request changes auth/redirect.ts.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    expect(screen.queryByText(/No files match/)).toBeNull();
  });
});
