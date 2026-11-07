import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../../api/pulls";
import type { PullRequest, Thread } from "../../api/types";
import { useConnection } from "../../state/connection";
import { PullChip, PullsRow } from "./PullChip";
import { useThreadPulls, WATCH_TIP } from "./pullStore";

const thread = { id: "t1", project_id: "p1" } as Thread;

const pull = (number: number, over: Partial<PullRequest> = {}): PullRequest => ({
  number,
  title: `Pull ${number}`,
  url: `https://github.com/acme/orbyn/pull/${number}`,
  state: "open",
  head: `feat/${number}`,
  base: "main",
  author: "ada",
  draft: false,
  updated_at: null,
  labels: [],
  review_state: null,
  checks_state: "pass",
  checks: { total: 4, passing: 4, failing: 0, running: 0, failing_names: [] },
  ...over,
});

const three = [
  pull(142, { checks_state: "fail", checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e"] } }),
  pull(3),
  pull(57, { draft: true, checks_state: "none", checks: undefined }),
];

function serve(pulls: PullRequest[], watched: number[] = []) {
  vi.spyOn(pullsApi, "linked").mockResolvedValue(pulls);
  vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue(pulls);
  vi.spyOn(pullsApi, "watches").mockResolvedValue(watched.map((number) => ({ thread_id: "t1", number, created_at: "" })));
  vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
}

beforeEach(() => {
  useThreadPulls.setState({ byThread: {} });
  useConnection.setState({ offline: false, serviceDown: false });
  window.location.hash = "";
});
afterEach(() => vi.restoreAllMocks());

const open = async () => {
  const user = userEvent.setup();
  render(<PullChip thread={thread} />);
  await user.click(await screen.findByRole("button", { name: "Pull requests" }));
  return user;
};

describe("pull request chip", () => {
  it("is not there when nothing is linked", async () => {
    serve([]);
    render(<PullChip thread={thread} />);
    await waitFor(() => expect(pullsApi.linked).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Pull requests" })).not.toBeInTheDocument();
  });

  it("reads #number for one pull request and PRs N for several, with a red dot when a check failed", async () => {
    serve([pull(142)]);
    const { unmount } = render(<PullChip thread={thread} />);
    expect(await screen.findByRole("button", { name: "Pull requests" })).toHaveTextContent("#142");
    unmount();
    useThreadPulls.setState({ byThread: {} });
    serve(three);
    render(<PullChip thread={thread} />);
    const chip = await screen.findByRole("button", { name: "Pull requests" });
    expect(chip).toHaveTextContent("PRs 3");
    expect(chip.querySelector(".pr-dot")).toHaveAttribute("data-tone", "red");
  });

  it("shows a blue dot while checks run", async () => {
    serve([pull(1, { checks_state: "pending", checks: { total: 4, passing: 1, failing: 0, running: 2, failing_names: [] } })]);
    render(<PullChip thread={thread} />);
    expect((await screen.findByRole("button", { name: "Pull requests" })).querySelector(".pr-dot")).toHaveAttribute("data-tone", "blue");
  });
});

describe("pull request popover", () => {
  it("lists the pull requests by number with their status, checks and branch, and puts focus on the first row", async () => {
    serve(three);
    await open();
    const dialog = await screen.findByRole("dialog", { name: "Pull requests" });
    expect(within(dialog).getByText("3 linked pull requests")).toBeInTheDocument();
    const rows = within(dialog).getAllByRole("link");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual(["#3 Pull 3", "#57 Pull 57", "#142 Pull 142"]);
    await waitFor(() => expect(rows[0]).toHaveFocus());
    expect(within(rows[0]!).getByText("Ready to merge")).toBeInTheDocument();
    expect(within(rows[0]!).getByText("4 of 4 checks passed")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("Draft")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("No checks")).toBeInTheDocument();
    expect(within(rows[2]!).getByText("1 of 4 checks failed")).toBeInTheDocument();
    expect(within(rows[2]!).queryByText("Ready to merge")).not.toBeInTheDocument();
  });

  it("opens the pull request page from a row", async () => {
    serve(three);
    const user = await open();
    await user.click(await screen.findByRole("link", { name: "#57 Pull 57" }));
    expect(window.location.hash).toBe("#/pulls/p1/57");
  });

  it("moves between rows with the arrow keys", async () => {
    serve(three);
    const user = await open();
    const rows = await screen.findAllByRole("link");
    await waitFor(() => expect(rows[0]).toHaveFocus());
    await user.keyboard("{ArrowDown}");
    expect(rows[1]).toHaveFocus();
  });

  it("turns Watch on and off at once, with the tooltip text", async () => {
    serve(three);
    vi.spyOn(pullsApi, "watch").mockResolvedValue(undefined);
    vi.spyOn(pullsApi, "unwatch").mockResolvedValue(undefined);
    const user = await open();
    const switches = await screen.findAllByRole("switch", { name: "Watch" });
    expect(switches[0]!.closest("label")).toHaveAttribute("title", WATCH_TIP);
    await user.click(switches[0]!);
    expect(switches[0]).toBeChecked();
    expect(pullsApi.watch).toHaveBeenCalledWith("t1", 3);
    await user.click(switches[0]!);
    expect(pullsApi.unwatch).toHaveBeenCalledWith("t1", 3);
  });

  it("refreshes from the forge and says how long ago it was read", async () => {
    serve(three);
    const user = await open();
    expect(await screen.findByText("Updated just now")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(pullsApi.refreshLinked).toHaveBeenCalledWith("t1");
  });

  it("keeps the list and says so when a refresh fails, with Try again", async () => {
    serve(three);
    const user = await open();
    await screen.findAllByRole("link");
    vi.spyOn(pullsApi, "refreshLinked").mockRejectedValue(new Error("no"));
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText(/GitHub didn’t answer\. Showing what was loaded just now\./)).toBeInTheDocument();
    expect(screen.getAllByRole("link")).toHaveLength(3);
    vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue(three);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText(/didn’t answer/)).not.toBeInTheDocument());
  });

  it("offline: shows what was loaded with no button, and mutes Refresh and Watch", async () => {
    serve(three);
    await open();
    await screen.findAllByRole("link");
    act(() => useConnection.setState({ offline: true }));
    expect(screen.getByText(/You’re offline\. Showing what was loaded/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
    for (const one of screen.getAllByRole("switch")) expect(one).toBeDisabled();
  });

  it("unlinks a pull request and reads the list again", async () => {
    serve(three);
    vi.spyOn(pullsApi, "unlink").mockResolvedValue(undefined);
    const user = await open();
    const row = (await screen.findAllByRole("link"))[0]!;
    await user.click(within(row).getByRole("button", { name: "Unlink" }));
    expect(pullsApi.unlink).toHaveBeenCalledWith("t1", 3);
  });

  it("links one by number from the footer", async () => {
    serve(three);
    vi.spyOn(pullsApi, "projectPulls").mockResolvedValue([pull(9)]);
    vi.spyOn(pullsApi, "link").mockResolvedValue(undefined);
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request" }));
    await user.click(await screen.findByRole("button", { name: "#9 Pull 9" }));
    expect(pullsApi.link).toHaveBeenCalledWith("t1", 9);
  });
});

describe("details card Pull requests row", () => {
  it("says how many, and opens the same list in place", async () => {
    serve(three);
    const user = userEvent.setup();
    render(<PullsRow thread={thread} />);
    const row = await screen.findByRole("button", { name: "Pull requests" });
    expect(row).toHaveTextContent("3 pull requests");
    await user.click(row);
    expect(await screen.findAllByRole("link")).toHaveLength(3);
    expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
  });

  it("says there is none yet, and offers to link one", async () => {
    serve([]);
    render(<PullsRow thread={thread} />);
    expect(await screen.findByText("No pull request yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link a pull request" })).toBeInTheDocument();
  });
});
