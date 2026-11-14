import { act, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../../../app/App";
import { REVIEW_FRAMES, type ReviewFrameId } from "./names";
import { openFrame } from "../../shared/test-support";
import { UNAVAILABLE_REASON } from "./Parts";

/** What each frame must show, from Crew Designs v19. */
const SHOWS: Record<ReviewFrameId, string[]> = {
  "risk-reasons": ["Risk tiers", "Merges by itself · review within 24 hours", "count as High unless the changes are in a path class marked Low in .orbyn/crew.json"],
  "risk-popover": ["Why Medium", "Area auth is on Full: 1 revert in 14 days · 8 merges"],
  "risk-high-full": ["Merged by Crew at 15:02.", "Why High", "Key rotation under load. No check covers it."],
  "risk-high-on": ["Waiting for you before the merge.", "Autopilot is On for this repo, and this change is High risk.", "Review", "Send back to Build", "Switch to Full", "Add reviewer", "Round 1", "Inbox · 3", "Planned: after the autonomy-by-area slice", "Revert rate"],
  "risk-raised": ["Raised from Medium", "Same engine as author · lead tier", "Sign in to another engine"],
  "risk-board-card": ["Low · Full", "High · On", "Waiting for you before the merge (On)"],
  "risk-demoted": ["Area auth is on On: 2 reverts in 14 days · 9 merges", "Where the area mode comes from"],
  "mergequeue-main": ["Re-checking combined", "Merging is code", "How the queue works"],
  "mergequeue-forecast": ["Conflict forecast", "Run them in order, the rename, then the move", "Why ordered, not parallel"],
  "mergequeue-removed": ["Left the queue", "What happens next"],
  "mergequeue-empty": ["Nothing in the merge queue"],
  "mergequeue-narrow-panel": ["Open in Crew", "Re-checking combined"],
  "review-queue": ["Rotate the signing key", "Ranked by risk, then by the time you have left", "Looks good for all 4", "156k tokens · 41 min", "50k tokens together"],
  "review-item": ["Not verified", "Reviewed by", "Paths outside the app are never followed."],
  "review-comment": ["Comment on this merge", "The brief Crew will write"],
  "review-revert": ["Revert this merge", "Later merges on top"],
  "review-revert-dependents": ["Another merge depends on this one", "Revert this one alone", "Revert both together"],
  "review-empty": ["Nothing to review", "Open the daily digest"],
  "review-overdue": ["1 review is overdue.", "Review overdue"],
  "review-overdue-high": ["1 High review is overdue.", "Its 4-hour window ended at 19:02, 1 h ago."],
  "review-reverted": ["Reverted automatically · the fix brief is queued", "Open the revert", "Create a check"],
  "review-batch": ["Batch review · 12 low-risk changes", "Untick all", "Looks good for 12", "12 changes · 160k tokens together · all checks passed"],
  "review-narrow-panel": ["Open in Crew", "top of queue"],
  "review-narrow-window": ["Rotate the signing key", "Use next in the auth redirect"],
  "review-demoted": ["Auth moved from Full to On at 15:36.", "Auth is on On: 2 reverts in 14 days"],
  "review-promoted": ["web/ earned Full on Sat 31 Oct, 10:00.", "Promoted: web is on Full"],
  "watch-broke": ["Main broke at 15:11.", "Merges since the last green main", "Main failed at 15:11 · watched since 09:00"],
  "watch-bisecting": ["Bisecting: tested 2 of at most 3 steps.", "Testing now", "Fails here"],
  "watch-reverted": ["Main is healthy again at 15:34.", "Why the cause was certain", "What happens to the original work"],
  "watch-asks": ["Crew isn’t certain why, so it is asking you.", "Revert the later one", "What Crew knows"],
  "watch-flow": ["Gate decided by:", "Click a line to open what it carries"],
  "watch-flaky": ["The failure didn’t reproduce", "Re-run the check", "What Crew saw"],
  "digest-daily": ["Waiting for your review", "Also today", "Costs are per engine, each in its own unit: Claude Code 412k tokens, Codex 62% of its weekly limit."],
  "digest-empty": ["Nothing merged today", "Open the Review queue"],
};

async function open(name: ReviewFrameId) {
  return openFrame(name);
}

describe("Autopilot review frames (Crew Designs v19)", () => {
  it("builds every frame of the slice", () => {
    expect(new Set(Object.keys(SHOWS))).toEqual(new Set(REVIEW_FRAMES));
    expect(REVIEW_FRAMES).toHaveLength(34);
  });

  it.each(REVIEW_FRAMES)("#/preview/crew/%s draws its state and makes no request", async (name) => {
    const { container, fetchSpy } = await open(name);
    for (const text of SHOWS[name]) expect(container.textContent).toContain(text);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it.each(REVIEW_FRAMES)("#/preview/crew/%s: every action that would change a repository is disabled and says why", async (name) => {
    const { container } = await open(name);
    const guarded = [...container.querySelectorAll<HTMLElement>("[aria-description]")];
    for (const control of guarded) {
      expect(control).toBeDisabled();
      expect(control).toHaveAttribute("aria-description", UNAVAILABLE_REASON);
    }
    for (const label of ["Looks good", "Approve and merge", "Revert both together", "Revert the later one", "Open the revert", "Create a check"]) {
      for (const button of screen.queryAllByRole("button", { name: label })) expect(button).toBeDisabled();
    }
  });

  it.each(REVIEW_FRAMES)("#/preview/crew/%s: no frame of the group offers Approve and merge; merging stays with the person-only review routes", async (name) => {
    const { container } = await open(name);
    expect(container.textContent).not.toContain("Approve and merge");
  });

  it("risk-high-on: Review opens the item in the Review queue, and is inert in the preview", async () => {
    await open("risk-high-on");
    const review = screen.getByRole("button", { name: "Review" });
    expect(review).toBeDisabled();
    expect(review).toHaveAttribute("aria-description", UNAVAILABLE_REASON);
    expect(screen.queryByRole("button", { name: "Approve and merge" })).toBeNull();
  });

  it("Crew v22: no frame shows a dollar figure, and the header says Codex's usage as a share of its weekly limit", async () => {
    for (const name of REVIEW_FRAMES) {
      const { container, unmount } = await open(name);
      expect(container.textContent, name).not.toMatch(/\$\d/);
      const usage = container.querySelector(".crew-end");
      if (usage) expect(usage.textContent, name).toBe("Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m");
      unmount();
    }
  });

  it("is absent without the preview flag's route: an unknown frame renders the normal app, not a Crew screen", async () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));
    window.location.hash = "#/preview/crew/not-a-frame";
    const { container } = render(<App />);
    await act(async () => {});
    expect(container.querySelector(".crew-root")).toBeNull();
    expect(within(container).queryByText("Sample data.")).toBeNull();
  });
});
