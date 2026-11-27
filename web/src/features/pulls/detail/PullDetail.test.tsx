import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../../../api/pulls";
import { ApiFailure } from "../../../api/client";
import { dataChanged } from "../../../api/shell";
import { COALESCE_MS, startDataRefresh } from "../../../lib/dataRefresh";
import type { PullCheck, PullRequestDetail } from "../../../api/types";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { setRunLinkClient } from "../../crew";
import { resetMergeOptions } from "../model/mergeOptions";
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
  // The detail loads first, then the extras read for its revision.
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
};

beforeEach(() => {
  vi.useFakeTimers();
  useConnection.setState({ offline: false, serviceDown: false });
  vi.spyOn(pullsApi, "detail");
  vi.spyOn(pullsApi, "checks").mockResolvedValue(checks);
  vi.spyOn(pullsApi, "breakdown").mockResolvedValue({ files: 1n, added: 1n, modified: 0n, deleted: 0n, renamed: 0n, additions: 3n, deletions: 0n });
  vi.spyOn(pullsApi, "linkedThreads").mockResolvedValue([]);
  vi.spyOn(pullsApi, "files").mockResolvedValue([
    { path: "auth/redirect.ts", previous_path: null, status: "modified", additions: 2, deletions: 1, patch: null, viewed: false, viewed_synced: false },
  ]);
  vi.spyOn(pullsApi, "details").mockResolvedValue({ details: [{ project_id: "p", number: 142, comment_count: 3 }], paused: [] } as never);
  vi.spyOn(pullsApi, "forges").mockResolvedValue([{ id: "github", name: "GitHub", program: "gh", installed: true, version: null, signed_in: true, accounts: ["me"], viewer: "me", problem: null }]);
  vi.spyOn(pullsApi, "stack").mockResolvedValue({ layers: [], position: 0 } as never);
  vi.spyOn(pullsApi, "reviewerCandidates").mockResolvedValue([]);
  vi.spyOn(pullsApi, "labelCandidates").mockResolvedValue([]);
  vi.spyOn(pullsApi, "update").mockResolvedValue(undefined);
  vi.spyOn(pullsApi, "mergeMethodSetting").mockResolvedValue(null);
  vi.spyOn(pullsApi, "timeline").mockResolvedValue([1, 2, 3].map((id) => ({ id: String(id), kind: "commented", actor: "sam", created_at: null, text: `Comment ${id}` })) as never);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const merge = () => screen.getByRole("button", { name: /^Merge$/ });

describe("one pull request as a page", () => {
  it.each([[0, "Approved"], [1, "1 approval"], [3, "3 approvals"], [undefined, "Approved"]] as const)("shows the known approval count %s (a known zero reads as unknown) in all three approved placements", async (count, words) => {
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, approval_count: count }], paused: [] });
    await show(detail({ review_state: "approved" }));
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent(words);
    expect(document.querySelector(".pulls-banner")).toHaveTextContent(`${words}. Checks passed and there are no conflicts.`);
    expect(document.querySelector(".pull-merge-hint")).toHaveTextContent(count ? `${words} · No conflicts` : "No conflicts");
    expect(merge()).toBeEnabled();
  });

  it.each([
    [{ draft: true }, "mergeable", "Not requested", false],
    [{ review_state: "changes_requested" }, "mergeable", "Changes requested", false],
    [{ review_state: "review_required" }, "mergeable", "Not requested", false],
    [{ review_state: "approved" }, "unknown", "2 approvals", true],
  ] as const)("keeps approval counts separate from precedence and merge eligibility %j", async (over, mergeable, words, enabled) => {
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, approval_count: 2 }], paused: [] });
    await show({ ...detail(over, mergeable), reviewers: [] });
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent(words);
    expect(merge().hasAttribute("disabled")).toBe(!enabled);
    expect(document.querySelector(".pull-merge-hint")?.textContent ?? "").not.toContain("approvals · No conflicts");
    expect(document.querySelector(".pulls-banner")?.textContent ?? "").not.toContain("approvals. Checks passed");
  });

  it("names the person's own requested changes on the merge row, and keeps Merge disabled", async () => {
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, viewer_review_state: "changes_requested" }], paused: [] });
    await show(detail({ review_state: "changes_requested" }));
    expect(document.querySelector(".pull-merge-hint")).toHaveTextContent("You requested changes. Merge needs an approval.");
    expect(merge()).toBeDisabled();
  });

  it("uses the current revision and forces a count read after approving", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, requested_reviewers: [{ name: "me", kind: "user" }], approval_count: 0 }], paused: [] });
    await show(detail({ review_state: "review_required", updated_at: "t1" }));
    // The read waits for the loaded revision: no request ever carries a null one.
    expect(vi.mocked(pullsApi.details).mock.calls.map(([body]) => body.pulls[0]?.updated_at)).toEqual(["t1"]);
    expect(pullsApi.details).toHaveBeenLastCalledWith({ pulls: [{ project_id: "p", number: 142, updated_at: "t1" }], fresh: false }, expect.any(AbortSignal));
    vi.mocked(pullsApi.detail).mockResolvedValue(detail({ review_state: "approved", updated_at: "t1" }));
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, approval_count: 1 }], paused: [] });
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(pullsApi.details).toHaveBeenLastCalledWith(expect.objectContaining({ fresh: true }), expect.any(AbortSignal));
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("1 approval");
  });

  it("keeps the unknown fallback while approval details are paused", async () => {
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [], paused: ["approval_count"] });
    await show(detail({ review_state: "approved" }));
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("Approved");
    expect(document.querySelector(".pulls-banner")).toHaveTextContent("Approved. Checks passed and there are no conflicts.");
    expect(document.querySelector(".pull-merge-hint")).toHaveTextContent("No conflicts");
    expect(merge()).toBeEnabled();
  });

  it("fences delayed details when the selected project and number change", async () => {
    let oldReply!: (value: Awaited<ReturnType<typeof pullsApi.details>>) => void;
    vi.mocked(pullsApi.detail).mockImplementation(async (projectId, number) => detail({ number, review_state: "approved", updated_at: projectId }));
    vi.mocked(pullsApi.details).mockImplementation(async (body) => body.pulls[0]?.project_id === "p"
      ? new Promise((resolve) => { oldReply = resolve; })
      : { details: [{ project_id: "q", number: 143, approval_count: 2 }], paused: [] });
    const view = render(<PullDetail layout="page" projectId="p" number={142} />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    view.rerender(<PullDetail layout="page" projectId="q" number={143} />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("2 approvals");
    await act(async () => oldReply({ details: [{ project_id: "p", number: 142, approval_count: 8 }], paused: [] }));
    expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("2 approvals");
    expect(pullsApi.details).toHaveBeenLastCalledWith({ pulls: [{ project_id: "q", number: 143, updated_at: "q" }], fresh: false }, expect.any(AbortSignal));
  });

  it("bypasses the details cache on a manual offline retry", async () => {
    useConnection.setState({ offline: true });
    await show(detail({ updated_at: "t1" }));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(pullsApi.details).toHaveBeenLastCalledWith({ pulls: [{ project_id: "p", number: 142, updated_at: "t1" }], fresh: true }, expect.any(AbortSignal));
  });

  it("keeps a fresh approval after a delayed background reply and rereads changed revisions", async () => {
    const stop = startDataRefresh();
    try {
      vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
      vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, requested_reviewers: [{ name: "me", kind: "user" }], approval_count: 0 }], paused: [] });
      await show(detail({ review_state: "review_required", updated_at: "t1" }));
      let oldReply!: (value: Awaited<ReturnType<typeof pullsApi.details>>) => void;
      vi.mocked(pullsApi.details).mockImplementationOnce(() => new Promise((resolve) => { oldReply = resolve; }));
      act(() => dataChanged("pull:p:142"));
      await act(async () => void (await vi.advanceTimersByTimeAsync(COALESCE_MS)));
      vi.mocked(pullsApi.detail).mockResolvedValue(detail({ review_state: "approved", updated_at: "t1" }));
      vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, approval_count: 1 }], paused: [] });
      fireEvent.click(screen.getByRole("button", { name: "Approve" }));
      await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
      expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("1 approval");
      await act(async () => oldReply({ details: [{ project_id: "p", number: 142, approval_count: 0 }], paused: [] }));
      expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("1 approval");
      vi.mocked(pullsApi.detail).mockResolvedValue(detail({ review_state: "approved", updated_at: "t2" }));
      vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, approval_count: 2 }], paused: [] });
      act(() => dataChanged("pull:p:142"));
      await act(async () => void (await vi.advanceTimersByTimeAsync(COALESCE_MS)));
      expect(pullsApi.details).toHaveBeenLastCalledWith({ pulls: [{ project_id: "p", number: 142, updated_at: "t2" }], fresh: false }, expect.any(AbortSignal));
      expect(screen.getByText("Review").nextElementSibling).toHaveTextContent("2 approvals");
    } finally {
      stop();
    }
  });

  it("names the failing check in a banner, links its log and blocks Merge", async () => {
    await show(detail({ checks_state: "fail", checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e (macOS)"] } }));
    const banner = screen.getByText("e2e (macOS) failed", { selector: "b" }).closest(".pulls-banner") as HTMLElement;
    expect(banner).toHaveTextContent("login ignores unsafe next values");
    expect(within(banner).getByRole("link", { name: "View log" })).toHaveAttribute("href", "https://example.com/log");
    expect(merge()).toBeDisabled();
    expect(screen.getByText("Merge is blocked by a failing check.")).toBeInTheDocument();
  });

  it("names a conflict in ink, saying only what the service says", async () => {
    await show(detail({ review_state: "approved" }, "conflicting"));
    const banner = screen.getByText("Conflicts with main", { selector: "b" }).closest(".pulls-banner") as HTMLElement;
    expect(banner).toHaveTextContent("Conflicts with main Merging is blocked.");
    expect(banner).not.toHaveAttribute("data-tone");
    expect(screen.getByText("Conflicts").nextElementSibling).toHaveTextContent("Yes");
    expect(screen.getByText("Conflicts").nextElementSibling).not.toHaveAttribute("data-tone");
    expect(merge()).toBeDisabled();
  });

  it("is amber for changes requested on the person's own pull request, and ink on someone else's", async () => {
    await show(detail({ review_state: "changes_requested", author: "me" }));
    const mine = document.querySelector(".pulls-banner") as HTMLElement;
    expect(mine).toHaveTextContent("Changes requested");
    expect(mine).toHaveAttribute("data-tone", "warn");
    expect(screen.getByText("Review").nextElementSibling).toHaveAttribute("data-tone", "warn");
    cleanup();
    await show(detail({ review_state: "changes_requested", author: "riley" }));
    expect(document.querySelector(".pulls-banner")).toHaveTextContent("Changes requested");
    expect(document.querySelector(".pulls-banner")).not.toHaveAttribute("data-tone");
    expect(screen.getByText("Review").nextElementSibling).not.toHaveAttribute("data-tone");
  });

  it("says Checks are still running under a Merge that waits for them", async () => {
    await show(detail({ checks_state: "pending", checks: { total: 4, passing: 2, failing: 0, running: 2, failing_names: [] } }));
    expect(screen.getByText("Checks are still running.")).toBeInTheDocument();
    expect(merge()).toBeDisabled();
  });

  it("offers Ready for review on a draft, which marks it ready", async () => {
    await show(detail({ draft: true }));
    expect(screen.getByText("Draft. Nobody is asked to review it yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ready for review" }));
    expect(pullsApi.update).toHaveBeenCalledWith("p", 142, { draft: false });
  });

  it("titles the Summary card only when the description does not open with that heading", async () => {
    await show({ ...detail(), body: "## Summary\n\n- one" });
    expect(document.querySelector(".pull-summary > b")).toBeNull();
    cleanup();
    await show({ ...detail(), body: "Fixes the redirect." });
    expect(document.querySelector(".pull-summary > b")).toHaveTextContent("Summary");
  });

  it("shapes the loading page like the frame: a title bar, two cards of lines and a side card, drawn after the wait", async () => {
    let finish: (value: PullRequestDetail) => void = () => {};
    await show(new Promise<PullRequestDetail>((resolve) => { finish = resolve; }));
    expect(document.querySelector(".pull-skel-card")).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(600)));
    const cards = [...document.querySelectorAll(".pull-skel-card")].map((card) => [...card.querySelectorAll("i")].map((line) => (line as HTMLElement).style.width));
    expect(cards).toEqual([["40%", "100%", "82%"], ["30%", "90%", "64%"], ["100%", "100%", "100%", "100%"]]);
    finish(detail());
  });

  it("keeps the page bar mounted from loading through an error and a retry to the loaded page", async () => {
    let finish: (value: PullRequestDetail) => void = () => {};
    vi.mocked(pullsApi.detail)
      .mockRejectedValueOnce(new ApiFailure("unreachable", "bad gateway", 502))
      .mockImplementationOnce(() => new Promise<PullRequestDetail>((resolve) => { finish = resolve; }));
    render(<PullDetail layout="page" projectId="p" number={142} />);
    const bar = document.querySelector(".page-bar") as HTMLElement;
    expect([...document.querySelectorAll(".pull-page-tabs button")].map((tab) => tab.textContent)).toEqual(["Summary", "Guide", "Conversation", "Files changed", "Checks"]);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(document.querySelector(".page-bar")).toBe(bar);
    bar.tabIndex = -1;
    bar.focus();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(document.querySelector(".page-bar")).toBe(bar);
    finish(detail());
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByRole("button", { name: /^Merge$/ })).toBeInTheDocument();
    expect(document.querySelector(".page-bar")).toBe(bar);
    expect(document.activeElement).toBe(bar);
  });

  it("shows the base branch as plain text in the Status card, with no input inside it", async () => {
    await show(detail());
    const card = document.querySelector(".pull-status") as HTMLElement;
    expect(card).toHaveTextContent("main");
    expect(card.querySelector("input, select")).toBeNull();
  });

  it("offers no base-branch change while the service reports no permission for it", async () => {
    await show(detail());
    fireEvent.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.queryByRole("menuitem", { name: "Change base branch" })).toBeNull();
    expect(document.querySelector(".pull-status input")).toBeNull();
    expect(document.querySelector("input[aria-label='Base branch']")).toBeNull();
  });

  it("renders the base branch field only after Change base branch is chosen, when the service allows it", async () => {
    await show({ ...detail(), can_change_base: true } as never);
    const card = document.querySelector(".pull-status") as HTMLElement;
    expect(card.querySelector("input")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "More actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Change base branch" }));
    expect(card.querySelector("input[aria-label='Base branch']")).toHaveValue("main");
  });

  it("reads the activity only when the Conversation tab opens, and keeps its own count after", async () => {
    // Five events against a comment_count of three, so the two sources cannot be mistaken for each other.
    vi.mocked(pullsApi.timeline).mockResolvedValue(
      [1, 2, 3, 4, 5].map((id) => ({ id: String(id), kind: "commented", actor: "sam", created_at: null, text: `Item ${id}` })) as never,
    );
    await show(detail());
    expect(pullsApi.timeline).not.toHaveBeenCalled();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toContain("Conversation · 3");
    fireEvent.click(screen.getByRole("tab", { name: /^Conversation/ }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(pullsApi.timeline).toHaveBeenCalledTimes(1);
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toContain("Conversation · 5");
    fireEvent.click(screen.getByRole("tab", { name: /^Summary/ }));
    // Back on another tab the count stays the activity's, not the comments'.
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toContain("Conversation · 5");
    expect(pullsApi.timeline).toHaveBeenCalledTimes(1);
  });

  it("counts the Conversation tab from everything it lists, commits included", async () => {
    vi.mocked(pullsApi.timeline).mockResolvedValue([
      { id: "a1", kind: "committed", actor: "Ada", created_at: "2026-11-05T01:05:00Z", text: "Fix the redirect" },
      { id: "b2", kind: "committed", actor: "Ada", created_at: "2026-11-05T01:06:00Z", text: "Add a test" },
      { id: "7", kind: "commented", actor: "sam", created_at: "2026-11-05T02:00:00Z", text: "Looks good" },
      { id: "8", kind: "reviewed", actor: "sam", created_at: "2026-11-05T03:00:00Z", text: "approved" },
    ] as never);
    await show({ ...detail(), pull: { ...detail().pull } });
    fireEvent.click(screen.getByRole("tab", { name: /^Conversation/ }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toContain("Conversation · 4");
  });

  it("counts files, checks and comments in the tab names", async () => {
    await show(detail());
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual(["Summary", "Guide", "Conversation · 3", "Files changed · 2", "Checks · 4"]);
  });

  it("shows the change breakdown and the linked threads on the Summary, and asks for both once", async () => {
    vi.mocked(pullsApi.linkedThreads).mockResolvedValue([{ id: "t9", project_id: "p", title: "Fix login redirect", engine: "claude", worktree: null, active_run: null, last_run_state: "completed", updated_at: new Date().toISOString() } as never]);
    await show(detail());
    expect(screen.getByText("Change breakdown")).toBeInTheDocument();
    expect(screen.getByText("Linked threads · 1")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Fix login redirect/ })).toBeInTheDocument();
    expect(pullsApi.breakdown).toHaveBeenCalledWith("p", 142, expect.anything());
    expect(pullsApi.linkedThreads).toHaveBeenCalledTimes(1);
  });

  it("chooses the merge method and options from the Merge method menu", async () => {
    await show(detail({ review_state: "approved" }));
    fireEvent.click(screen.getByRole("button", { name: /Squash and merge/ }));
    const menu = screen.getByRole("menu", { name: "Merge method" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Squash and merge", "Create a merge commit", "Rebase and merge"]);
    expect(within(menu).getAllByRole("menuitemcheckbox").map((item) => item.textContent)).toEqual(["Merge when checks pass", "Delete branch after merge"]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Rebase and merge" }));
    expect(screen.getByRole("button", { name: /Rebase and merge/ })).toBeInTheDocument();
  });

  it("starts the merge menu and its dialog on the project's merge method", async () => {
    vi.mocked(pullsApi.mergeMethodSetting).mockResolvedValue("rebase");
    await show(detail({ review_state: "approved" }));
    expect(pullsApi.mergeMethodSetting).toHaveBeenCalledWith("p", expect.anything());
    expect(screen.getByRole("button", { name: /Rebase and merge/ })).toBeInTheDocument();
    fireEvent.click(merge());
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    // The page's menu chose the method, so the dialog only says which.
    expect(screen.getByText("Rebase and merge #142 into main.")).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Merge method" })).toBeNull();
  });

  it("waits for the project's merge method before the page dialog opens", async () => {
    let release: (value: string | null) => void = () => undefined;
    const setting = new Promise<string | null>((resolve) => { release = resolve; });
    vi.mocked(pullsApi.mergeMethodSetting).mockReturnValue(setting);
    await show(detail({ review_state: "approved" }));
    // The setting is still loading, so Merge waits for it instead of opening on the fallback.
    fireEvent.click(merge());
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.queryByText("Merge pull request")).toBeNull();
    await act(async () => { release("rebase"); await Promise.resolve(); });
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByText("Rebase and merge #142 into main.")).toBeInTheDocument();
  });

  it("shares Merge when checks pass and Delete branch after merge between the menu and the dialog", async () => {
    vi.spyOn(pullsApi, "creditsSetting").mockResolvedValue(false);
    resetMergeOptions();
    await show(detail({ review_state: "approved" }));
    fireEvent.click(screen.getByRole("button", { name: /Squash and merge/ }));
    // The two options are checkboxes with a plain check when on, and choosing one closes the menu.
    fireEvent.click(within(screen.getByRole("menu", { name: "Merge method" })).getByRole("menuitemcheckbox", { name: "Merge when checks pass" }));
    expect(screen.queryByRole("menu", { name: "Merge method" })).toBeNull();
    fireEvent.click(merge());
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByRole("checkbox", { name: "Merge when checks pass" })).toBeChecked();
    // Ticking in the dialog shows ticked in the menu.
    fireEvent.click(screen.getByRole("checkbox", { name: "Delete the branch" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: /Squash and merge/ }));
    const menu = screen.getByRole("menu", { name: "Merge method" });
    expect(within(menu).getByRole("menuitemcheckbox", { name: "Merge when checks pass" })).toHaveAttribute("aria-checked", "true");
    expect(within(menu).getByRole("menuitemcheckbox", { name: "Delete branch after merge" })).toHaveAttribute("aria-checked", "true");
    expect(menu.querySelectorAll(".menu-check")).toHaveLength(2);
  });

  it("says it is offline, with Retry, and Merge waits for a connection", async () => {
    useConnection.setState({ offline: true });
    await show(detail());
    expect(screen.getByText(/You’re offline\. Showing what was loaded/)).toBeInTheDocument();
    expect(screen.getByText("Merge needs a connection.")).toBeInTheDocument();
    expect(merge()).toBeDisabled();
  });

  it("says the details did not load when nothing is cached, and Try again asks again", async () => {
    vi.mocked(pullsApi.detail).mockRejectedValue(new ApiFailure("bad_gateway", "no", 502));
    render(<PullDetail layout="page" projectId="p" number={142} />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByRole("heading", { name: "Pull request #142" })).toBeInTheDocument();
    expect(screen.getByText("Couldn’t load details.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
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

  it("shows GitHub's own viewed state for the files and marks a file", async () => {
    vi.mocked(pullsApi.files).mockResolvedValue([
      { path: "auth/login.ts", previous_path: null, status: "modified", additions: 1, deletions: 0, patch: null, viewed: true, viewed_synced: true },
      { path: "auth/redirect.ts", previous_path: null, status: "modified", additions: 2, deletions: 1, patch: null, viewed: false, viewed_synced: false },
    ]);
    const setViewed = vi.spyOn(pullsApi, "setViewed").mockResolvedValue(undefined);
    await show(detail());
    fireEvent.click(screen.getByRole("tab", { name: /^Files changed/ }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent("Viewed 1 of 2");
    expect(screen.getByRole("checkbox", { name: "Viewed auth/login.ts" })).toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "Viewed auth/redirect.ts" }));
    expect(setViewed).toHaveBeenCalledWith("p", 142, ["auth/redirect.ts"], true);
  });

  it("keeps a mark GitHub did not save, says so, and re-reads the files", async () => {
    const unmarked = { path: "auth/redirect.ts", previous_path: null, status: "modified", additions: 2, deletions: 1, patch: null, viewed: false, viewed_synced: false };
    const kept = { ...unmarked, viewed: true, viewed_synced: false };
    vi.mocked(pullsApi.files).mockResolvedValueOnce([unmarked]).mockResolvedValue([kept]);
    vi.spyOn(pullsApi, "setViewed").mockRejectedValue(
      new ApiFailure("viewed_unsynced", "GitHub did not save the viewed mark for auth/redirect.ts. It is kept in Orbyn. HTTP 403: Resource not accessible by integration", 502),
    );
    useUi.setState({ toasts: [] });
    await show(detail());
    fireEvent.click(screen.getByRole("tab", { name: /^Files changed/ }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    fireEvent.click(screen.getByRole("checkbox", { name: "Viewed auth/redirect.ts" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual([
      "Couldn't mark that. GitHub did not save the viewed mark for auth/redirect.ts. It is kept in Orbyn. HTTP 403: Resource not accessible by integration",
    ]);
    expect(pullsApi.files).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("checkbox", { name: "Viewed auth/redirect.ts" })).toBeChecked();
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent("Viewed 1 of 1");
  });

  it("says how many checks failed above the Checks tab, in place of the state banner", async () => {
    await show(detail({ checks_state: "fail", checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e (macOS)"] } }));
    fireEvent.click(screen.getByRole("tab", { name: /^Checks/ }));
    expect(screen.getByText("1 check failed")).toBeInTheDocument();
    expect(screen.queryByText("e2e (macOS) failed", { selector: "b" })).toBeNull();
  });

  it("asks before closing, and closes only on the confirmation", async () => {
    vi.spyOn(pullsApi, "action").mockResolvedValue(undefined);
    await show(detail());
    fireEvent.click(screen.getByRole("button", { name: "More actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Close pull request" }));
    expect(pullsApi.action).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("dialog", { hidden: true })).getByRole("button", { name: "Close pull request" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(pullsApi.action).toHaveBeenCalledWith("p", 142, "close");
  });

  it("reads the review request again after Approve, so the banner goes", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, requested_reviewers: [{ name: "me", kind: "user" }], comment_count: 3 }], paused: [] } as never);
    await show(detail({ review_state: "review_required" }));
    expect(screen.getByText("Your review is requested")).toBeInTheDocument();
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, requested_reviewers: [], comment_count: 3 }], paused: [] } as never);
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.queryByText("Your review is requested")).toBeNull();
  });

});

describe("Request changes", () => {
  const asked = () =>
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [{ project_id: "p", number: 142, requested_reviewers: [{ name: "me", kind: "user" }], comment_count: 3 }], paused: [] } as never);
  const open = async (over: Record<string, unknown> = {}) => {
    asked();
    await show(detail({ review_state: "review_required", ...over }));
    fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
  };
  const note = () => screen.getByRole("textbox", { name: "What needs to change?" });
  const card = () => within(screen.getByRole("region", { name: "Request changes" }));
  const send = () => card().getByRole("button", { name: /^(Request changes|Try again)$/ });
  const flushAll = () => act(async () => void (await vi.advanceTimersByTimeAsync(0)));

  it("opens a card under the merge row, and sends only once there is a note", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    await open();
    expect(screen.getByText("Your note is sent to the author.")).toBeInTheDocument();
    expect(send()).toBeDisabled();
    fireEvent.change(note(), { target: { value: "Reject //evil.com" } });
    expect(send()).toBeEnabled();
    fireEvent.click(send());
    await flushAll();
    expect(pullsApi.review).toHaveBeenCalledWith("p", 142, "request_changes", "Reject //evil.com");
  });

  it("shows a spinner and Requesting… while it waits, with the note and Cancel locked", async () => {
    vi.spyOn(pullsApi, "review").mockReturnValue(new Promise(() => {}));
    await open();
    fireEvent.change(note(), { target: { value: "x" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByRole("button", { name: "Requesting…" })).toBeDisabled();
    expect(note()).toBeDisabled();
    expect(card().getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("becomes a timeline entry, labels the title, and says so on the merge row", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    await open();
    fireEvent.change(note(), { target: { value: "Fix this" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByRole("region", { name: "You requested changes" })).toBeInTheDocument();
    expect(screen.getByText("Fix this")).toBeInTheDocument();
    expect(screen.getByText("Changes requested", { selector: ".pull-changes-label" })).toBeInTheDocument();
    expect(screen.getByText("You requested changes. Merge needs an approval.")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "What needs to change?" })).toBeNull();
  });

  it("keeps the note and says nothing was sent when the forge refuses", async () => {
    vi.spyOn(pullsApi, "review").mockRejectedValue(new ApiFailure("git_failed", "boom", 409));
    await open();
    fireEvent.change(note(), { target: { value: "Keep me" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByText("Couldn’t submit the review. Nothing was sent.")).toBeInTheDocument();
    expect(note()).toHaveValue("Keep me");
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
  });

  it("names the forge when it cannot take a review", async () => {
    vi.spyOn(pullsApi, "review").mockRejectedValue(new ApiFailure("unsupported", "no", 422));
    await open();
    fireEvent.change(note(), { target: { value: "x" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByText("GitHub doesn’t provide that here.")).toBeInTheDocument();
  });

  it("will not request changes on your own pull request, and Approve is off too", async () => {
    asked();
    await show(detail({ review_state: "review_required", author: "me" }));
    expect(screen.getByText("You opened this pull request.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
    expect(screen.getByText("You can’t review your own pull request.")).toBeInTheDocument();
    expect(send()).toBeDisabled();
  });

  it("asks before throwing away a note on Escape, and closes at once with none", async () => {
    await open();
    fireEvent.keyDown(note(), { key: "Escape" });
    expect(screen.queryByRole("textbox", { name: "What needs to change?" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
    fireEvent.change(note(), { target: { value: "draft" } });
    fireEvent.keyDown(note(), { key: "Escape" });
    expect(note()).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole("dialog", { hidden: true })).getByRole("button", { name: "Discard" }));
    await flushAll();
    expect(screen.queryByRole("textbox", { name: "What needs to change?" })).toBeNull();
  });

  it("keeps the Request changes button in the banner, pressed, while the card is open", async () => {
    await open();
    expect(screen.getAllByRole("button", { name: "Request changes" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Request changes" })[0]).toHaveAttribute("aria-pressed", "true");
  });

  it("closes from any control of the card on Escape, and asks first when there is a note", async () => {
    await open();
    fireEvent.keyDown(card().getByRole("button", { name: "Cancel" }), { key: "Escape" });
    expect(screen.queryByRole("region", { name: "Request changes" })).toBeNull();
  });

  it("keeps the note when the card is closed and opened again", async () => {
    await open();
    fireEvent.change(note(), { target: { value: "Remember me" } });
    fireEvent.click(card().getByRole("button", { name: "Cancel" }));
    fireEvent.click(within(screen.getByRole("dialog", { hidden: true })).getByRole("button", { name: "Discard" }));
    await flushAll();
    fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
    expect(note()).toHaveValue("Remember me");
  });

  it("dims the merge row while the card is open, and expands the checks again once it is sent", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    await open();
    expect(document.querySelector(".pull-merge")).toHaveAttribute("data-dim");
    expect(screen.queryByText("lint")).toBeNull();
    fireEvent.change(note(), { target: { value: "x" } });
    fireEvent.click(send());
    await flushAll();
    expect(document.querySelector(".pull-merge")).not.toHaveAttribute("data-dim");
    expect(screen.getByText("lint")).toBeInTheDocument();
  });

  it("puts the sent note in the left column as an entry, and the label as a neutral pill", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    await open();
    fireEvent.change(note(), { target: { value: "Fix this" } });
    fireEvent.click(send());
    await flushAll();
    const entry = screen.getByRole("region", { name: "You requested changes" });
    expect(entry.closest(".pull-main")).not.toBeNull();
    expect(entry).toHaveTextContent("You requested changes · just now");
    expect(screen.getByText("Changes requested", { selector: ".pull-changes-label" })).toBeInTheDocument();
  });

  it("says Changes requested by you in the Review row, and counts the review in the Conversation tab", async () => {
    vi.spyOn(pullsApi, "review").mockResolvedValue(undefined);
    asked();
    await show({ ...detail({ review_state: "review_required" }), reviewers: ["me"] });
    fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
    expect(screen.getByText("Requested from you")).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toContain("Conversation · 3");
    // Once the review is in, the conversation lists it as a fourth entry.
    vi.mocked(pullsApi.timeline).mockResolvedValue([1, 2, 3, 4].map((id) => ({ id: String(id), kind: "commented", actor: "sam", created_at: null, text: `Item ${id}` })) as never);
    fireEvent.change(note(), { target: { value: "Fix this" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByText("Changes requested by you")).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toContain("Conversation · 4");
    expect(document.querySelector(".pull-status b[data-tone=warn]")).toBeNull();
  });

  it("says the review may have been sent when the forge did not answer, and reads the reviews again", async () => {
    vi.spyOn(pullsApi, "review").mockRejectedValue(new ApiFailure("bad_gateway", "no", 504));
    await open();
    const reads = vi.mocked(pullsApi.detail).mock.calls.length;
    fireEvent.change(note(), { target: { value: "x" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByText(/It may have been sent/)).toBeInTheDocument();
    expect(vi.mocked(pullsApi.detail).mock.calls.length).toBeGreaterThan(reads);
  });
});

describe("the Run tab", () => {
  afterEach(() => {
    setRunLinkClient(null);
    useConnection.setState({ crewPreview: false });
    vi.unstubAllGlobals();
  });

  it("is absent while Crew Preview is off, so no Crew route is asked", async () => {
    await show(detail());
    expect(screen.queryByRole("tab", { name: "Run" })).toBeNull();
  });

  it("is drawn from the service's own run link when Crew Preview is on", async () => {
    const fetch = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ task: "task-redirect" }), { status: 200 })));
    vi.stubGlobal("fetch", fetch);
    useConnection.setState({ crewPreview: true });
    await show(detail());
    expect(fetch).toHaveBeenCalledWith("/api/crew/flow/run-link?project=p&number=142", expect.anything());
    expect(screen.getByRole("tab", { name: "Run" })).toBeInTheDocument();
  });

  it("is drawn when Crew Preview turns on while the page is open", async () => {
    const fetch = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ task: "task-redirect" }), { status: 200 })));
    vi.stubGlobal("fetch", fetch);
    await show(detail());
    expect(screen.queryByRole("tab", { name: "Run" })).toBeNull();
    expect(fetch).not.toHaveBeenCalled();
    act(() => useConnection.setState({ crewPreview: true }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(fetch).toHaveBeenCalledWith("/api/crew/flow/run-link?project=p&number=142", expect.anything());
    expect(screen.getByRole("tab", { name: "Run" })).toBeInTheDocument();
  });

  it("is absent when the service says no Crew task made the pull request", async () => {
    const fetch = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ code: "not_found", message: "no task made it" }), { status: 404 })));
    vi.stubGlobal("fetch", fetch);
    useConnection.setState({ crewPreview: true });
    await show(detail());
    expect(screen.queryByRole("tab", { name: "Run" })).toBeNull();
  });

  it("is drawn only when the client says a Crew task made the pull request", async () => {
    const forPull = vi.fn().mockResolvedValue({ task: "task-redirect" });
    setRunLinkClient({ forPull });
    await show(detail());
    expect(forPull).toHaveBeenCalledWith("p", 142, expect.anything());
    expect(screen.getByRole("tab", { name: "Run" })).toBeInTheDocument();
  });
});
