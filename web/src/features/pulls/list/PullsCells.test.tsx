import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../../../api/pulls";
import type { PullDetailsResponse, PullListEntry } from "../../../api/types";
import { MOTION } from "../../../lib/motion";
import { PullsPage } from "./PullsPage";

const entry = (number: number, over: Partial<PullListEntry["pull"]> = {}, state = "open"): PullListEntry => ({
  project_id: "p",
  pull: {
    number,
    title: `PR ${number}`,
    url: "https://github.com/o/r/pull/" + number,
    state,
    head: "feat",
    base: "main",
    author: "ada",
    draft: false,
    updated_at: new Date(Date.now() - 3_600_000).toISOString(),
    labels: [],
    review_state: null,
    checks_state: "pass",
    checks: { total: 4, passing: 4, failing: 0, running: 0, failing_names: [] },
    ...over,
  },
});

const forge = { id: "github", name: "GitHub", program: "gh", installed: true, version: null, signed_in: true, accounts: ["me"], viewer: "me", problem: null };
const details = (number: number, over: Record<string, unknown> = {}): PullDetailsResponse["details"][number] =>
  ({ project_id: "p", number, ...over }) as PullDetailsResponse["details"][number];

let answer!: (response: PullDetailsResponse) => void;
let refuse!: () => void;

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(pullsApi, "forges").mockResolvedValue([forge]);
  vi.spyOn(pullsApi, "list").mockImplementation(async (state) => ({
    pulls: state === "closed" ? [entry(9, {}, "closed")] : [entry(1, { checks: { total: 5, passing: 2, failing: 3, running: 0, failing_names: ["e2e", "lint", "docs"] } })],
    failed: [],
  }));
  vi.spyOn(pullsApi, "details").mockImplementation(
    () =>
      new Promise<PullDetailsResponse>((resolve, reject) => {
        answer = resolve;
        refuse = () => reject(new Error("no"));
      }),
  );
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const row = () => document.querySelector(".pull-rows > li") as HTMLElement;
const settle = () => act(async () => void (await vi.advanceTimersByTimeAsync(0)));

describe("a row's details cells", () => {
  it("fills the cells from the list and the details, with the checks word first", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () =>
      answer({
        details: [
          details(1, {
            requested_reviewers: [
              { name: "riley", kind: "user" },
              { name: "web", kind: "team" },
              { name: "sam", kind: "user" },
            ],
            comment_count: 12,
            viewer_review_state: "approved",
          }),
        ],
        paused: [],
      }),
    );
    const cells = within(row());
    expect(cells.getByText("Failing · e2e +2")).toBeInTheDocument();
    expect(cells.getByText("+1")).toBeInTheDocument();
    expect(cells.getByText("You approved")).toBeInTheDocument();
    expect(cells.getByText("12 comments, including review comments", { selector: "[role=tooltip]" })).toBeInTheDocument();
    expect(cells.getByText("riley, @web, sam", { selector: "[role=tooltip]" })).toBeInTheDocument();
  });

  it("leaves Approved or Changes after the avatars, amber only for changes on the person's own pull request", async () => {
    vi.mocked(pullsApi.list).mockResolvedValue({
      pulls: [entry(1, { review_state: "approved" }), entry(2, { review_state: "changes_requested" }), entry(3, { review_state: "changes_requested", author: "me" })],
      failed: [],
    });
    render(<PullsPage />);
    await settle();
    const reviewers = { requested_reviewers: [{ name: "riley", kind: "user" }], comment_count: 0, viewer_review_state: "none" };
    await act(async () => answer({ details: [details(1, reviewers), details(2, reviewers), details(3, reviewers)], paused: [] }));
    const words = [...document.querySelectorAll(".pull-rword")].map((word) => [word.textContent, word.getAttribute("data-tone")]);
    expect(words).toEqual([["Approved", "quiet"], ["Changes", "quiet"], ["Changes", "act"]]);
  });

  it("shows nothing for 400 ms from the request, then quiet bars that are replaced by the values", async () => {
    render(<PullsPage />);
    await settle();
    expect(row().querySelectorAll(".pull-pending")).toHaveLength(0);
    act(() => void vi.advanceTimersByTime(MOTION.wait));
    expect(row().querySelectorAll(".pull-pending")).toHaveLength(2);
    expect(row().querySelectorAll(".pull-skel-circles i")).toHaveLength(2);
    await act(async () => answer({ details: [details(1, { requested_reviewers: [], comment_count: 3, viewer_review_state: "commented" })], paused: [] }));
    expect(row().querySelectorAll(".pull-pending")).toHaveLength(0);
    expect(within(row()).getByText("You commented")).toBeInTheDocument();
  });

  it("shows an en dash with the reason, and the header offers Retry, when details cannot load", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => refuse());
    expect(within(row()).getAllByText("—")).toHaveLength(3);
    expect(within(row()).getAllByText("Couldn’t load details", { selector: "[role=tooltip]" })).toHaveLength(3);
    expect(screen.getByText(/Details unavailable/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry loading details" }));
    expect(pullsApi.details).toHaveBeenCalledTimes(2);
    expect(vi.mocked(pullsApi.details).mock.calls[1]?.[0].fresh).toBe(true);
  });

  it("says the rate limit paused comment counts instead of showing a count", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [], viewer_review_state: "none" })], paused: ["comment_count"] }));
    expect(within(row()).getByText("Comment counts are paused until GitHub’s rate limit resets", { selector: "[role=tooltip]" })).toBeInTheDocument();
  });

  it("names the forge when it does not provide a field", async () => {
    vi.mocked(pullsApi.list).mockResolvedValue({ pulls: [entry(1, { url: "https://gitlab.com/o/r/-/merge_requests/1", checks: undefined })], failed: [] });
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1)], paused: [] }));
    const tips = within(row()).getAllByRole("tooltip", { hidden: true }).map((tip) => tip.textContent);
    expect(tips).toContain("GitLab doesn’t provide check counts here.");
  });
});

describe("a filter that needs details", () => {
  /** Reviews are asked, so Needs my review is offered; then the Closed list brings rows without details. */
  const chooseThenSwitchList = async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [{ name: "me", kind: "user" }] })], paused: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Needs my review" }));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Closed/ }));
    await settle();
  };

  it("keeps the rows as they are for 400 ms, then shows the loading rows with a spinner on its chip", async () => {
    await chooseThenSwitchList();
    expect(screen.getByRole("button", { name: /PR 9/ })).toBeInTheDocument();
    expect(document.querySelectorAll(".pull-skel > li")).toHaveLength(0);
    act(() => void vi.advanceTimersByTime(MOTION.wait));
    expect(document.querySelectorAll(".pull-skel > li")).toHaveLength(6);
    expect(document.querySelector(".pulls-fchip .pulls-wait")).not.toBeNull();
    expect(document.querySelector(".pulls-table")).toHaveAttribute("aria-busy", "true");
  });

  it("falls back to the unfiltered rows with a banner, and Remove filter takes the chip away", async () => {
    await chooseThenSwitchList();
    await act(async () => refuse());
    expect(screen.getByRole("button", { name: /PR 9/ })).toBeInTheDocument();
    expect(screen.getByText("Couldn’t load details, so this filter can’t apply yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove filter" }));
    expect(screen.queryByText("Couldn’t load details, so this filter can’t apply yet.")).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove filter Review: needs my review" })).toBeNull();
  });

  it("keeps rows whose details are missing when a filter needs details", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [{ name: "me", kind: "user" }] })], paused: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Needs my review" }));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Closed/ }));
    await settle();
    await act(async () => refuse());
    expect(screen.getByRole("button", { name: /PR 9/ })).toBeInTheDocument();
  });

  it("when the rate limit pauses reviewers, shows every row, a neutral banner without Retry and a paused chip, then asks again at the reset", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [{ name: "me", kind: "user" }] })], paused: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Needs my review" }));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Closed/ }));
    await settle();
    const resumeAt = new Date(Date.now() + 60_000).toISOString();
    await act(async () => answer({ details: [], paused: ["requested_reviewers"], resume_at: resumeAt } as never));
    expect(screen.getByRole("button", { name: /PR 9/ })).toBeInTheDocument();
    expect(screen.getByText(/GitHub is limiting requests, so the reviewer and check filters are paused\. They resume at \d\d:\d\d\./)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(screen.getByRole("tooltip", { name: "Paused" })).toBeInTheDocument();
    const calls = vi.mocked(pullsApi.details).mock.calls.length;
    await act(async () => void (await vi.advanceTimersByTimeAsync(61_000)));
    expect(vi.mocked(pullsApi.details).mock.calls.length).toBe(calls + 1);
  });

  const limitedList = async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [{ name: "me", kind: "user" }] })], paused: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Needs my review" }));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Closed/ }));
    await settle();
  };

  it("when the reset passes and the forge still limits, the banner says so with the next time, and goes when a check succeeds", async () => {
    await limitedList();
    await act(async () => answer({ details: [], paused: ["requested_reviewers"], resume_at: new Date(Date.now() + 30_000).toISOString() } as never));
    await act(async () => void (await vi.advanceTimersByTimeAsync(31_000)));
    // The check at the reset is a plain read, not every row asked for fresh.
    expect(vi.mocked(pullsApi.details).mock.calls.at(-1)?.[0].fresh).toBe(false);
    // The banner stays while the check is out.
    expect(screen.getByText(/is limiting requests/)).toBeInTheDocument();
    await act(async () => answer({ details: [], paused: ["requested_reviewers"], resume_at: new Date(Date.now() + 120_000).toISOString() } as never));
    expect(screen.getByText(/GitHub is still limiting requests, so the reviewer and check filters stay paused\. Still limited\. Checking again at \d\d:\d\d\./)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(121_000)));
    await act(async () => answer({ details: [details(9, { requested_reviewers: [] })], paused: [] }));
    expect(screen.queryByText(/limiting requests/)).toBeNull();
  });

  it("checks again after 60 seconds when the service gives no reset time, and does not start the wait over on a render", async () => {
    await limitedList();
    await act(async () => answer({ details: [], paused: ["requested_reviewers"] }));
    const calls = vi.mocked(pullsApi.details).mock.calls.length;
    await act(async () => void (await vi.advanceTimersByTimeAsync(40_000)));
    // Anything that renders the page in the meantime must not push the check back.
    fireEvent.change(screen.getByRole("textbox", { name: "Search pull requests" }), { target: { value: "x" } });
    await act(async () => void (await vi.advanceTimersByTimeAsync(21_000)));
    expect(vi.mocked(pullsApi.details).mock.calls.length).toBe(calls + 1);
  });

  it("says they resume soon when the service gives no time", async () => {
    render(<PullsPage />);
    await settle();
    await act(async () => answer({ details: [details(1, { requested_reviewers: [{ name: "me", kind: "user" }] })], paused: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Needs my review" }));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Closed/ }));
    await settle();
    await act(async () => answer({ details: [], paused: ["requested_reviewers"] }));
    expect(screen.getByText(/They resume soon\./)).toBeInTheDocument();
  });
});

describe("details in chunks", () => {
  it("asks for at most 100 rows a request", async () => {
    vi.mocked(pullsApi.list).mockResolvedValue({ pulls: Array.from({ length: 230 }, (_, at) => entry(at + 1)), failed: [] });
    render(<PullsPage />);
    await settle();
    expect(vi.mocked(pullsApi.details).mock.calls.map(([body]) => body.pulls.length)).toEqual([100, 100, 30]);
  });
});

describe("one details chunk failing", () => {
  it("keeps the rows the other chunks answered and says details are unavailable for the rest", async () => {
    vi.mocked(pullsApi.list).mockResolvedValue({ pulls: Array.from({ length: 150 }, (_, at) => entry(at + 1)), failed: [] });
    vi.mocked(pullsApi.details).mockImplementation(async (body) => {
      if (body.pulls.length === 50) throw new Error("no");
      return { details: body.pulls.map((one) => details(Number(one.number), { requested_reviewers: [], comment_count: 5, viewer_review_state: "none" })), paused: [] };
    });
    render(<PullsPage />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    const rows = document.querySelectorAll(".pull-rows > li");
    expect(rows[0]!.querySelector(".pull-count")).not.toBeNull();
    expect(rows[120]!.querySelector(".pull-count")).toBeNull();
    expect(screen.getByText(/Details unavailable/)).toBeInTheDocument();
  });

  it("offers Merge on a row only when it is Ready to merge, and not while offline", async () => {
    vi.mocked(pullsApi.list).mockResolvedValue({ pulls: [entry(1, { review_state: "approved" }), entry(2, { review_state: "changes_requested" })], failed: [] });
    vi.mocked(pullsApi.details).mockResolvedValue({ details: [], paused: [] });
    render(<PullsPage />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    expect(screen.getAllByRole("button", { name: /^Merge #/ }).map((one) => one.getAttribute("aria-label"))).toEqual(["Merge #1"]);
  });
});
