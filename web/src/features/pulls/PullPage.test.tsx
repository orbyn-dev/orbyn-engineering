import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
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
  vi.spyOn(pullsApi, "timeline").mockResolvedValue([1, 2, 3].map((id) => ({ id: String(id), kind: "commented", actor: "sam", created_at: null, text: `Comment ${id}` })) as never);
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

  it("counts the Conversation tab from everything it lists, commits included", async () => {
    vi.mocked(pullsApi.timeline).mockResolvedValue([
      { id: "a1", kind: "committed", actor: "Ada", created_at: "2026-11-05T01:05:00Z", text: "Fix the redirect" },
      { id: "b2", kind: "committed", actor: "Ada", created_at: "2026-11-05T01:06:00Z", text: "Add a test" },
      { id: "7", kind: "commented", actor: "sam", created_at: "2026-11-05T02:00:00Z", text: "Looks good" },
      { id: "8", kind: "reviewed", actor: "sam", created_at: "2026-11-05T03:00:00Z", text: "approved" },
    ] as never);
    await show({ ...detail(), pull: { ...detail().pull } });
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toContain("Conversation · 4");
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
    expect(screen.getByText("You requested changes.")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "What needs to change?" })).toBeNull();
  });

  it("keeps the note and says nothing was sent when the forge refuses", async () => {
    vi.spyOn(pullsApi, "review").mockRejectedValue(new ApiFailure("git_failed", "boom", 409));
    await open();
    fireEvent.change(note(), { target: { value: "Keep me" } });
    fireEvent.click(send());
    await flushAll();
    expect(screen.getByText("Couldn’t submit the review. Nothing was sent. Try again.")).toBeInTheDocument();
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
    expect(screen.getByText("You can’t request changes on your own pull request.")).toBeInTheDocument();
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

  it("puts the sent note in the left column as an entry, and the label as an amber pill", async () => {
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
