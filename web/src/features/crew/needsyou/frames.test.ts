import { describe, expect, it } from "vitest";
import { describeFrames } from "../shared/test-support";
import { NEEDSYOU_FRAMES, NEEDSYOU_NOT_BUILT } from "./names";

const DRAWN = NEEDSYOU_FRAMES.filter((name) => name !== "needsyou-live");

const NOTE = "Risk and reversibility only where the item’s data has them";
const INBOX = ["Needs you 4", NOTE];

/** What each frame must show: text taken from its design (Crew Designs v37, group 9). */
const SHOWS: Record<string, string[]> = {
  "needsyou-inbox": [...INBOX, "Denied 1", "Questions 1", "Main 1", "Conflicts 1", "A request only you can approve was denied: publish the package", "Claude Code · Sonnet 5.5 · person-only", "The task went on without it. Nothing is blocked; do it yourself when you choose.", "Open the request", "Dismiss", "Two workers disagree about the redirect rule", "Waiting holds 2 tasks. The choice is yours; the evidence is side by side.", "3 questions from OpenCode, batched", "Answer 3", "Main broke and the cause isn’t certain", "Watch on main · 9be4410 and f1a0c63 together", "Nothing has been reverted. Main stays as it is until you choose.", "Re-run the check", "Inbox · 4"],
  "needsyou-inbox-off": [...INBOX, "Approvals 1", "Questions 1", "Reviews 1", "Conflicts 1", "Approve: run “npm publish --dry-run” in auth/", "Waiting holds 1 task and its lease on auth/redirect.ts. Nothing else is blocked.", "Approve", "Deny", "Review “Use next in the auth redirect”", "Risk · Medium", "Easy to undo", "Autopilot is Off · every change waits for you", "Waiting costs nothing now, and the change stays as it is.", "Open review", "Later"],
  "needsyou-approval": [...INBOX, "Approvals 1", "Approve: run “npm publish --dry-run” in auth/", "Approve a command", "Claude Code · Sonnet 5.5 asks to run a command in its worktree crew/redirect-impl.", "npm publish --dry-run --workspace auth", "Waiting holds", "1 task and its lease on auth/redirect.ts", "Approve once"],
  "needsyou-empty": ["Inbox", "Nothing needs you", "When something needs you, it appears here.", "Open the Flow view"],
  "needsyou-many": ["Inbox · 27", "9 workers", "All projects 27", "orbyn-engineering 21", "orbyn-docs 6", "Showing 4 of 27 · the rest are grouped below", "Answer: which package manager?", "Review: licence file added", "Risk · Medium", "Approve: rm -rf build/", "Holds 2 tasks.", "Usage page crew", "Docs refresh crew"],
  "needsyou-narrow-panel": ["Inbox 4", "Approval", "Run “npm publish --dry-run” in auth/", "Claude Code · Sonnet 5.5", "Waiting holds 1 task and 1 lease", "Conflict", "Waiting holds 2 tasks", "Questions", "3 questions from OpenCode", "Answer 3"],
  "needsyou-budget": ["How the inbox is ranked", "Crew puts what matters most at the top and batches what can wait, so the person is interrupted less and never surprised.", "1 · Risk", "“A change waiting for your review”: High", "2 · Reversibility", "3 · Cost of waiting", "Batching", "What you control", "High-risk items (irreversible, security, money or data loss) interrupt at once"],
  "needsyou-demoted": [...INBOX, "Autonomy 1", "Auth moved from Full to On: 2 reverts in 14 days. Review or override.", "Autonomy by area · circuit breaker · 9 merges in the window", "Nothing is blocked. A High risk change in auth/ now waits for you before it merges; Low and Medium still merge by themselves.", "Review", "Override…"],
  "needsyou-stop": ["Needs you 2", "Stops 2", NOTE, "Stopped · too many denials", "Dismiss hides this stop on this device.", "Dismiss", "Login redirect crew · Worker 2 · Claude Code · Sonnet 5.5", "3 denials in a row stopped the task. Nothing else is blocked; the other tasks keep going.", "Stopped · possible injected instructions", "A page the worker read contained text that looks like instructions. The task stopped before it ran anything the text asked for.", "Open"],
  "needsyou-background": ["Needs you 2", "Background 2", NOTE, "Paused: budget reached", "Background crew · Check dependencies weekly · Claude Code", "The run stopped at its budget. Raise the budget, or leave it paused.", "Needs approval", "Background crew · Update the docs index · Sonnet 5.5", "A command is held for your approval before the run can go on.", "Open"],
  "needsyou-source-error": ["Needs you 2", "Questions 1", "Main 1", NOTE, "Can’t read held commands. The rest is shown.", "One source of the inbox did not answer", "Try again", "3 questions from OpenCode, batched", "Main broke and the cause isn’t certain"],
};

describeFrames("Needs you frames (Crew Designs, group 9)", DRAWN, SHOWS);

describe("the frames of the group", () => {
  it.each(DRAWN)("#/preview/crew/%s draws a chip only on a review row, and no row is amber", async (name) => {
    const { openFrame } = await import("../shared/test-support");
    const { container } = await openFrame(name);
    const rows = [...container.querySelectorAll(".ny-item, .ny-card")];
    for (const row of rows) {
      expect(row).not.toHaveAttribute("data-hi");
      if (row.getAttribute("data-kind") !== "review") expect(row.querySelector(".ny-tag")).toBeNull();
    }
    expect(container.textContent).not.toContain("Approve for this crew");
    if (name !== "needsyou-inbox-off" && name !== "needsyou-approval" && name !== "needsyou-many") expect(container.querySelector(".ny-tag")).toBeNull();
  });

  it("draws the source error as an unnumbered row above the rows, which keep their numbers", async () => {
    const { openFrame } = await import("../shared/test-support");
    const { container } = await openFrame("needsyou-source-error");
    const rows = [...container.querySelectorAll(".ny-item")];
    expect(rows.map((row) => row.querySelector(".ny-rk")?.textContent)).toEqual(["", "1", "2"]);
    expect(rows[0]).toHaveTextContent("Can’t read held commands. The rest is shown.");
  });

  it("names the two it does not build, with nothing drawn for them", () => {
    expect(NEEDSYOU_NOT_BUILT).toEqual(["needsyou-batch", "needsyou-conflict"]);
    for (const name of NEEDSYOU_NOT_BUILT) expect(NEEDSYOU_FRAMES as readonly string[]).not.toContain(name);
  });
});
