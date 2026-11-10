import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../api/client";
import { pullsApi } from "../../api/pulls";
import type { PullRequest, Thread } from "../../api/types";
import { useConnection } from "../../product/state/connection";
import { PullChip, PullsRow } from "./PullChip";
import { useThreadPulls } from "../pulls";
import { WATCH_TIP } from "../../product/pulls/labels";

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
    const rows = within(dialog).getAllByRole("button", { name: /^#\d+ / });
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
    await user.click(await screen.findByRole("button", { name: "#57 Pull 57" }));
    expect(window.location.hash).toBe("#/pulls/p1/57");
  });

  it("moves between rows with the arrow keys", async () => {
    serve(three);
    const user = await open();
    const rows = await screen.findAllByRole("button", { name: /^#\d+ / });
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
    await screen.findAllByRole("button", { name: /^#\d+ / });
    vi.spyOn(pullsApi, "refreshLinked").mockRejectedValue(new Error("no"));
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText(/GitHub didn’t answer\. Showing what was loaded just now\./)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^#\d+ / })).toHaveLength(3);
    vi.spyOn(pullsApi, "refreshLinked").mockResolvedValue(three);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText(/didn’t answer/)).not.toBeInTheDocument());
  });

  it("offline: shows what was loaded with no button, and mutes Refresh and Watch", async () => {
    serve(three);
    await open();
    await screen.findAllByRole("button", { name: /^#\d+ / });
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
    await user.click(await screen.findByRole("button", { name: "Unlink #3" }));
    expect(pullsApi.unlink).toHaveBeenCalledWith("t1", 3);
    await waitFor(() => expect(pullsApi.linked).toHaveBeenCalledTimes(3));
  });

  it("puts every control in the Tab order: the row, its switch and Unlink are siblings", async () => {
    serve(three);
    await open();
    const row = (await screen.findAllByRole("button", { name: /^#\d+ / }))[0]!.closest(".pr-row")!;
    expect(row.querySelector("[role=link]")).toBeNull();
    for (const control of row.querySelectorAll("button, input")) expect(control.closest("button, [role=link]")).toBe(control.tagName === "BUTTON" ? control : null);
    expect([...row.querySelectorAll("button, input")].every((control) => (control as HTMLElement).tabIndex >= 0)).toBe(true);
  });

  it("links one by number from the footer, and says when there is no such pull request", async () => {
    serve(three);
    vi.spyOn(pullsApi, "link").mockRejectedValueOnce(new ApiFailure("not_found", "gone", 404)).mockResolvedValue(undefined);
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.type(screen.getByRole("textbox", { name: "Pull request number" }), "9");
    await user.click(screen.getByRole("button", { name: "Link" }));
    expect(await screen.findByText("No pull request #9 in this repository.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Link" }));
    await waitFor(() => expect(pullsApi.link).toHaveBeenLastCalledWith("t1", 9));
  });

  it("writes a GitLab merge request as !number, and names GitLab when it does not answer", async () => {
    serve([pull(3, { url: "https://gitlab.com/acme/orbyn/-/merge_requests/3" })]);
    const user = await open();
    expect(screen.getByRole("button", { name: "Pull requests" })).toHaveTextContent("!3");
    expect(await screen.findByRole("button", { name: "!3 Pull 3" })).toBeInTheDocument();
    vi.spyOn(pullsApi, "refreshLinked").mockRejectedValue(new Error("no"));
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText(/GitLab didn’t answer\./)).toBeInTheDocument();
  });

  it("keeps the chip, an error with Retry and Link reachable when the first read fails", async () => {
    vi.spyOn(pullsApi, "linked").mockRejectedValueOnce(new Error("no")).mockRejectedValueOnce(new Error("no")).mockResolvedValue(three);
    vi.spyOn(pullsApi, "watches").mockResolvedValue([]);
    vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
    const user = userEvent.setup();
    render(<PullChip thread={thread} />);
    await user.click(await screen.findByRole("button", { name: "Pull requests" }));
    const dialog = await screen.findByRole("dialog", { name: "Pull requests" });
    await waitFor(() => expect(within(dialog).getByText("The host didn’t answer.")).toBeInTheDocument());
    expect(within(dialog).getByRole("button", { name: "Link a pull request…" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Retry" }));
    expect(await within(dialog).findAllByRole("button", { name: /^#\d+ / })).toHaveLength(3);
  });

  it("returns focus to the Details button when the last pull request is unlinked", async () => {
    serve([pull(1)]);
    vi.spyOn(pullsApi, "unlink").mockImplementation(async () => {
      vi.mocked(pullsApi.linked).mockResolvedValue([]);
    });
    const user = userEvent.setup();
    render(
      <>
        <button data-details-toggle>Details</button>
        <PullChip thread={thread} />
      </>,
    );
    await user.click(await screen.findByRole("button", { name: "Pull requests" }));
    await user.click(await screen.findByRole("button", { name: "Unlink #1" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Details" })).toHaveFocus());
  });
});

describe("pull request list focus and link errors", () => {
  it("moves focus to the next row after a middle row is unlinked, and to the previous row when the last of several goes", async () => {
    serve(three);
    vi.spyOn(pullsApi, "unlink").mockImplementation(async (_thread, number) => {
      vi.mocked(pullsApi.linked).mockResolvedValue(three.filter((entry) => entry.number !== number));
    });
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Unlink #57" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "#142 Pull 142" })).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "Unlink #142" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "#3 Pull 3" })).toHaveFocus());
  });

  it("returns focus to Link a pull request… when the form is cancelled", async () => {
    serve(three);
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Link a pull request…" })).toHaveFocus());
  });

  it("returns focus to Link a pull request… after a pull request is linked", async () => {
    serve(three);
    vi.spyOn(pullsApi, "link").mockResolvedValue(undefined);
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.type(screen.getByRole("textbox", { name: "Pull request number" }), "9");
    await user.click(screen.getByRole("button", { name: "Link" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Link a pull request…" })).toHaveFocus());
  });

  it("says there is no such pull request only for a 404; a server error is the host not answering, with Retry", async () => {
    serve(three);
    vi.spyOn(pullsApi, "link").mockRejectedValueOnce(new ApiFailure("boom", "boom", 500)).mockRejectedValueOnce(new ApiFailure("gone", "gone", 404));
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.type(screen.getByRole("textbox", { name: "Pull request number" }), "9");
    await user.click(screen.getByRole("button", { name: "Link" }));
    expect(await screen.findByText("GitHub didn’t answer.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("No pull request #9 in this repository.")).toBeInTheDocument();
  });

  it("shows the service's own message for another 4xx, such as a 422 or a 400, with no Retry and no claim that the pull request is missing", async () => {
    serve(three);
    vi.spyOn(pullsApi, "link")
      .mockRejectedValueOnce(new ApiFailure("unsupported", "Install gh and sign in to it.", 422))
      .mockRejectedValueOnce(new ApiFailure("bad", "This project has no origin remote.", 400));
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.type(screen.getByRole("textbox", { name: "Pull request number" }), "9");
    await user.click(screen.getByRole("button", { name: "Link" }));
    expect(await screen.findByText("Install gh and sign in to it.")).toBeInTheDocument();
    expect(screen.queryByText(/No pull request/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Link" }));
    expect(await screen.findByText("This project has no origin remote.")).toBeInTheDocument();
    expect(screen.queryByText(/didn’t answer/)).not.toBeInTheDocument();
  });

  it("rejects a number that is not a safe integer, and sends nothing", async () => {
    serve(three);
    const link = vi.spyOn(pullsApi, "link").mockResolvedValue(undefined);
    const user = await open();
    await user.click(await screen.findByRole("button", { name: "Link a pull request…" }));
    await user.type(screen.getByRole("textbox", { name: "Pull request number" }), "9007199254740993");
    await user.click(screen.getByRole("button", { name: "Link" }));
    expect(await screen.findByText("Enter a pull request number.")).toBeInTheDocument();
    expect(link).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox", { name: "Pull request number" })).toHaveAttribute("maxlength", "17");
  });
});

describe("pull request cache", () => {
  it("keeps the entry the details row shares when the chip unmounts, and drops it with the last consumer", async () => {
    serve(three);
    const chip = render(<PullChip thread={thread} />);
    const row = render(<PullsRow thread={thread} />);
    await screen.findAllByRole("button", { name: "Pull requests" });
    chip.unmount();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(useThreadPulls.getState().byThread["t1"]?.pulls).toHaveLength(3);
    row.unmount();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(useThreadPulls.getState().byThread["t1"]).toBeUndefined();
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
    expect(await screen.findAllByRole("button", { name: /^#\d+ / })).toHaveLength(3);
    expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
  });

  it("says there is none yet, and offers to link one", async () => {
    serve([]);
    render(<PullsRow thread={thread} />);
    expect(await screen.findByText("No pull request yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link by number" })).toBeInTheDocument();
  });

  it("says the host did not answer and offers Try again", async () => {
    vi.spyOn(pullsApi, "linked").mockRejectedValue(new Error("no"));
    vi.spyOn(pullsApi, "watches").mockResolvedValue([]);
    render(<PullsRow thread={thread} />);
    expect(await screen.findByText("The host didn’t answer")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });
});

describe("pull request store", () => {
  it("ignores a read that lands after a newer one, and clears busy only when the last ends", async () => {
    let slow!: (value: PullRequest[]) => void;
    vi.spyOn(pullsApi, "linked")
      .mockImplementationOnce(() => new Promise((resolve) => (slow = resolve)))
      .mockResolvedValue([pull(2)]);
    vi.spyOn(pullsApi, "watches").mockResolvedValue([]);
    vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
    const first = useThreadPulls.getState().load(thread);
    await useThreadPulls.getState().load(thread);
    expect(useThreadPulls.getState().byThread["t1"]?.pulls?.map((entry) => entry.number)).toEqual([2]);
    expect(useThreadPulls.getState().byThread["t1"]?.busy).toBe(true);
    slow([pull(1)]);
    await first;
    expect(useThreadPulls.getState().byThread["t1"]?.pulls?.map((entry) => entry.number)).toEqual([2]);
    expect(useThreadPulls.getState().byThread["t1"]?.busy).toBe(false);
  });

  it("drops a thread's entry when its chip goes", async () => {
    serve([pull(1)]);
    const { unmount } = render(<PullChip thread={thread} />);
    await screen.findByRole("button", { name: "Pull requests" });
    unmount();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(useThreadPulls.getState().byThread["t1"]).toBeUndefined();
  });
});
