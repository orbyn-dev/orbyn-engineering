import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../../api/pulls";
import type { PullDetailsResponse, PullListEntry } from "../../api/types";
import { MOTION } from "../../lib/motion";
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
    expect(tips).toContain("GitLab doesn’t provide check counts");
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
});
