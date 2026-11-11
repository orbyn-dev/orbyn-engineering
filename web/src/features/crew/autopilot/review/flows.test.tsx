import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../../../app/App";
import { Act, UNAVAILABLE_REASON } from "./Parts";
import { BatchReview } from "./Batch";
import { ReviewItem } from "./ReviewItem";
import { ReviewQueue, queueCounts } from "./ReviewQueue";
import { batch, queue, queueOverdue, queueReverted } from "./sampleQueue";
import { mergeItem, mergeItemWithDependents } from "./sampleMerge";

async function route(name: string) {
  window.location.hash = `#/preview/crew/${name}`;
  const view = render(<App />);
  await act(async () => {});
  await screen.findByText(/Sample data\./);
  return view;
}

describe("Review queue", () => {
  it("counts add up: the tab count is the sum of the items, with a batch counted by its members", async () => {
    expect(queueCounts(queue)).toMatchObject({ "to-review": 7, high: 1, overdue: 0, reverted: 1 });
    await route("review-queue");
    expect(screen.getByRole("tab", { name: "Review · 7" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Review queue · 7" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "To review 7" })).toHaveAttribute("aria-pressed", "true");
  });

  it("the filters narrow the list, and a filter with nothing in it is disabled", async () => {
    await route("review-queue");
    expect(screen.getByRole("button", { name: "Overdue 0" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "High 1" }));
    const list = screen.getByRole("list", { name: "Merges to review" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
    expect(within(list).getByText("Rotate the signing key")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "To review 7" }));
    expect(within(list).getAllByRole("listitem")).toHaveLength(4);
  });

  it("a reverted merge stays in the list, dimmed, with a link to the revert, and is not counted as waiting", () => {
    render(<ReviewQueue data={queueReverted} onOpen={vi.fn()} />);
    const card = screen.getByRole("article", { name: /Rename the legacy returnTo helper/ });
    expect(within(card).getByText("Reverted automatically · the fix brief is queued")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Revert" })).toBeNull();
    expect(within(card).getByRole("button", { name: "Open the revert" })).toBeDisabled();
  });

  it("an overdue item is amber, says how late, and is a reminder with no block on its actions", () => {
    render(<ReviewQueue data={queueOverdue} onOpen={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("1 review is overdue.");
    const card = screen.getByRole("article", { name: /Move session helpers/ });
    expect(within(card).getByText("Review overdue")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Comment" })).toBeEnabled();
  });

  it("Comment and Revert on a card open the item with that dialog; nothing is sent from the card", async () => {
    const open = vi.fn();
    render(<ReviewQueue data={queue} onOpen={open} />);
    const card = screen.getByRole("article", { name: /Use next in the auth redirect/ });
    await userEvent.click(within(card).getByRole("button", { name: "Revert" }));
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ id: "redirect-impl" }), "revert");
    await userEvent.click(within(card).getByRole("button", { name: "Comment" }));
    expect(open).toHaveBeenLastCalledWith(expect.objectContaining({ id: "redirect-impl" }), "comment");
  });

  it("Looks good acts only through the handler it was given", async () => {
    const good = vi.fn();
    render(<ReviewQueue data={queue} onOpen={vi.fn()} onLooksGood={good} />);
    const card = screen.getByRole("article", { name: /Use next in the auth redirect/ });
    await userEvent.click(within(card).getByRole("button", { name: "Looks good" }));
    expect(good).toHaveBeenCalledWith(expect.objectContaining({ id: "redirect-impl" }));
  });
});

describe("a guarded action", () => {
  it("with no handler is disabled and carries the reason; with one it runs", async () => {
    const run = vi.fn();
    const { rerender } = render(<Act label="Go" />);
    const off = screen.getByRole("button", { name: "Go" });
    expect(off).toBeDisabled();
    expect(off).toHaveAttribute("aria-description", UNAVAILABLE_REASON);
    rerender(<Act label="Go" onAct={run} />);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(run).toHaveBeenCalledTimes(1);
  });
});

describe("opened merge", () => {
  it("Revert asks first: the handler runs only from the dialog's confirm, never from the Revert button", async () => {
    const revert = vi.fn();
    render(<ReviewItem item={mergeItem} actions={{ revert }} />);
    await userEvent.click(screen.getByRole("button", { name: "Revert" }));
    expect(revert).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Revert this merge" });
    expect(within(dialog).getByText(/Main isn’t changed until the revert passes its checks/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Open the revert" }));
    expect(revert).toHaveBeenCalledWith("It breaks the legacy login link.", false);
  });

  it("Cancel and Escape leave without acting", async () => {
    const revert = vi.fn();
    render(<ReviewItem item={mergeItem} actions={{ revert }} />);
    await userEvent.click(screen.getByRole("button", { name: "Revert" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Revert" }));
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(revert).not.toHaveBeenCalled();
  });

  it("with later merges on top it lists them and asks, offering one alone or both together", async () => {
    const revert = vi.fn();
    render(<ReviewItem item={mergeItemWithDependents} initialDialog="revert" actions={{ revert }} />);
    const dialog = screen.getByRole("dialog", { name: "Another merge depends on this one" });
    expect(within(dialog).getByText(/f1a0c63 · Add a rate limit to the login route/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Revert both together" }));
    expect(revert).toHaveBeenCalledWith("It breaks the legacy login link.", true);
  });

  it("Comment sends the text as a fix brief only on confirm", async () => {
    const sendBrief = vi.fn();
    render(<ReviewItem item={mergeItem} actions={{ sendBrief }} />);
    await userEvent.click(screen.getByRole("button", { name: "Comment" }));
    const field = screen.getByLabelText("What should change?");
    await userEvent.clear(field);
    await userEvent.type(field, "Reject backslashes.");
    expect(sendBrief).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Send as a fix brief" }));
    expect(sendBrief).toHaveBeenCalledWith("Reject backslashes.");
  });

  it("not verified is always on the page", () => {
    render(<ReviewItem item={mergeItem} />);
    expect(screen.getByRole("region", { name: "Not verified" })).toHaveTextContent("Safari and Firefox behaviour");
  });
});

describe("batch review", () => {
  it("unticking a row takes it out of the count, and Untick all empties the batch", async () => {
    render(<BatchReview data={batch} />);
    expect(screen.getByRole("button", { name: "Looks good for 12" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("checkbox", { name: /Update the login docs/ }));
    expect(screen.getByRole("button", { name: "Looks good for 11" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Untick all" }));
    expect(screen.getByRole("button", { name: "Looks good for 6" })).toBeInTheDocument();
    expect(screen.getAllByRole("checkbox").every((box) => !(box as HTMLInputElement).checked)).toBe(true);
  });
});

describe("keyboard and roles", () => {
  it("the tool tabs are a tablist and move between the Autopilot views", async () => {
    await route("review-queue");
    const tabs = screen.getByRole("tablist", { name: "Autopilot views" });
    expect(within(tabs).getByRole("tab", { selected: true })).toHaveTextContent("Review queue · 7");
    await userEvent.click(within(tabs).getByRole("tab", { name: "Merge queue · 3" }));
    expect(window.location.hash).toBe("#/preview/crew/mergequeue-main");
    const next = await screen.findByRole("tablist", { name: "Autopilot views" });
    await userEvent.click(within(next).getByRole("tab", { name: "Digest" }));
    expect(window.location.hash).toBe("#/preview/crew/digest-daily");
  });

  it("the tier popover opens from its chip and closes on Escape, returning focus to the chip", async () => {
    await route("risk-popover");
    const chip = screen.getByRole("button", { name: "Medium" });
    expect(chip).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("dialog", { name: "Why Medium" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(chip).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("dialog", { name: "Why Medium" })).toBeInTheDocument();
  });

  it("a working row announces that it is working", async () => {
    await route("mergequeue-main");
    expect(screen.getByRole("img", { name: "Working" })).toBeInTheDocument();
  });

  it("the Review screen opens the item from a card title, and the item goes back to the queue", async () => {
    await route("review-queue");
    await userEvent.click(screen.getByRole("button", { name: "Use next in the auth redirect" }));
    expect(window.location.hash).toBe("#/preview/crew/review-item");
    await act(async () => {});
    await userEvent.click(await screen.findByRole("button", { name: "Back to the queue" }));
    expect(window.location.hash).toBe("#/preview/crew/review-queue");
  });
});
