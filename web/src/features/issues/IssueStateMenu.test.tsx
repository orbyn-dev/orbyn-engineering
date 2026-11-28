import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ApiFailure } from "../../api/client";
import { useUi } from "../../product/state/ui";
import type { IssueDetail, IssueState } from "./contract";
import type { IssueWriter } from "./IssueEdit";
import { IssueStateButton } from "./IssueStateMenu";

/**
 * The State row's one-write guard (IssueStateMenu), pinned without timing: the component is rendered on its own, with an `issue`
 * prop the page never updates, so the window between the write answering and the issue showing the new state stays open for as
 * long as the test needs. The `write` seam answers only when the test says so, so the second choice lands in that window by
 * construction rather than by a sleep or a suite under load (issue #1799).
 */

const OPEN: IssueState = { category: "open", name: "Open" };
const COMPLETED: IssueState = { category: "closed", name: "Closed", close_reason: "completed" };
const NOT_PLANNED: IssueState = { category: "closed", name: "Closed as not planned", close_reason: "not_planned" };

/** One issue's page, as the page passes it down. Identity is kept between renders, as the page's own state keeps it. */
const ISSUE: IssueDetail = {
  tracker: "github",
  scope: "orbyn-dev/orbyn-engineering",
  key: "412",
  display_key: "#412",
  title: "Login sends people back to /home",
  url: "https://github.com/orbyn-dev/orbyn-engineering/issues/412",
  author: { login: "riley" },
  state: OPEN,
  created_at: "2026-11-20T10:00:00Z",
  updated_at: "2026-11-21T10:00:00Z",
  closed_at: null,
  assignees: [],
  labels: [],
  comment_count: 0,
  body: "After sign-in the app opens /home.",
  comments: [],
  linked_pull_requests: [],
  linked_threads: [],
  states: [OPEN, COMPLETED, NOT_PLANNED],
};

/**
 * The page's write guard, held open: the write stays in flight until the test hands it an answer, so which click lands on which
 * side of the answer is decided by the test, not by how fast the suite is running. `call` is never run here: this file pins which
 * click is allowed to reach the write, and the request `call` builds is http.test.ts's.
 */
function heldWriter() {
  const waiting: { answer(answered: IssueDetail | null): void; fail(failure: unknown): void }[] = [];
  const write = vi.fn<IssueWriter>(() => new Promise<IssueDetail | null>((resolve, reject) => {
    waiting.push({ answer: resolve, fail: reject });
  }));
  const answer = async (answered: IssueDetail | null) => { await act(async () => { waiting.shift()?.answer(answered); }); };
  const fail = async (failure: unknown) => { await act(async () => { waiting.shift()?.fail(failure); }); };
  return { write, answer, fail };
}

const stateButton = (word = "Open") => screen.getByRole("button", { name: `State: ${word}` });
const item = (name: string) => within(screen.getByRole("menu", { name: "Change the state" })).getByRole("menuitem", { name });
const openMenu = (shown = "Open") => fireEvent.click(stateButton(shown));
/** A whole choice: open the row's menu, then pick a state in it. */
const choose = (name: string, shown = "Open") => { openMenu(shown); fireEvent.click(item(name)); };

it("a second choice of the same state, answered but not yet re-rendered, writes nothing", async () => {
  const { write, answer } = heldWriter();
  render(<IssueStateButton issue={ISSUE} write={write} writing={false} />);
  choose("Closed");
  expect(write).toHaveBeenCalledTimes(1);
  // The write answers while the issue still shows Open: the window the claim exists for.
  await answer({ ...ISSUE, state: COMPLETED });
  choose("Closed");
  expect(write).toHaveBeenCalledTimes(1);
});

it("a different choice while a write is in flight is not a second write", () => {
  const { write } = heldWriter();
  render(<IssueStateButton issue={ISSUE} write={write} writing={false} />);
  openMenu();
  const closed = item("Closed");
  const notPlanned = item("Closed as not planned");
  fireEvent.click(closed);
  // The menu is leaving, and its copy is still the one a second press lands on: the claim does not cover another state.
  expect(closed.isConnected).toBe(true);
  fireEvent.click(notPlanned);
  expect(write).toHaveBeenCalledTimes(1);
});

it("a writer refusal releases the claim, so the same choice can be retried", async () => {
  const { write, answer } = heldWriter();
  render(<IssueStateButton issue={ISSUE} write={write} writing={false} />);
  choose("Closed");
  // The page's guard answers null while it is locked: nothing changed, and nothing was asked of the tracker.
  await answer(null);
  choose("Closed");
  expect(write).toHaveBeenCalledTimes(2);
});

it("a failed write releases the claim, so the same choice can be retried", async () => {
  const { write, fail } = heldWriter();
  render(<IssueStateButton issue={ISSUE} write={write} writing={false} />);
  choose("Closed");
  await fail(new ApiFailure("tracker_unavailable", "The tracker said no.", 502));
  expect(useUi.getState().toasts.some((toast) => /Couldn.t change the state\. Nothing changed\./.test(toast.text))).toBe(true);
  choose("Closed");
  expect(write).toHaveBeenCalledTimes(2);
});

it("once the issue shows a state, a claimed choice is released and writes again", async () => {
  const { write, answer } = heldWriter();
  const { rerender } = render(<IssueStateButton issue={ISSUE} write={write} writing={false} />);
  choose("Closed");
  expect(write).toHaveBeenCalledTimes(1);
  await answer({ ...ISSUE, state: COMPLETED });
  // The page then shows the issue as the tracker answered: another state than the one asked for, which still clears the claim.
  rerender(<IssueStateButton issue={{ ...ISSUE, state: NOT_PLANNED }} write={write} writing={false} />);
  choose("Closed", "Closed as not planned");
  expect(write).toHaveBeenCalledTimes(2);
});
