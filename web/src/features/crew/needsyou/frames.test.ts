import { describe, expect, it } from "vitest";
import { describeFrames } from "../shared/test-support";
import { NEEDSYOU_FRAMES, NEEDSYOU_NOT_BUILT } from "./names";

const DRAWN = NEEDSYOU_FRAMES.filter((name) => name !== "needsyou-live");

const INBOX = ["Needs you 4", "Ranked by risk, then by how hard it is to undo, then by what waiting costs"];

/** What each frame must show: text taken from its design (Crew Designs, group 9). */
const SHOWS: Record<string, string[]> = {
  "needsyou-inbox": [...INBOX, "Denied 1", "Questions 1", "Main 1", "Conflicts 1", "A request only you can approve was denied: publish the package", "Risk · High", "Hard to undo", "Claude Code · Sonnet 5.5 · person-only", "The task went on without it. Nothing is blocked; do it yourself when you choose.", "Open the request", "Dismiss", "Two workers disagree about the redirect rule", "Waiting holds 2 tasks. The choice is yours; the evidence is side by side.", "3 questions from OpenCode, batched", "Answer 3", "Main broke and the cause isn’t certain", "Watch on main · 9be4410 and f1a0c63 together", "Nothing has been reverted. Main stays as it is until you choose.", "Re-run the check", "Inbox · 4"],
  "needsyou-inbox-off": [...INBOX, "Approvals 1", "Questions 1", "Reviews 1", "Conflicts 1", "Approve: run “npm publish --dry-run” in auth/", "Waiting holds 1 task and its lease on auth/redirect.ts. Nothing else is blocked.", "Approve", "Deny", "Approve and merge “Use next in the auth redirect”", "Autopilot is Off · every change waits for you", "Waiting costs nothing now, and the PR stays open.", "Open review", "Later"],
  "needsyou-approval": [...INBOX, "Approvals 1", "Approve: run “npm publish --dry-run” in auth/", "Approve", "Deny"],
  "needsyou-empty": ["Inbox", "Nothing needs you", "When something needs you, it appears here, ranked, with what waiting costs.", "Open the Flow view"],
  "needsyou-many": ["Inbox · 27", "9 workers", "All projects 27", "orbyn-engineering 21", "orbyn-docs 6", "Ranked by risk, then by how hard it is to undo, then by what waiting costs", "Answer: which package manager?", "Review: licence file added", "Approve: rm -rf build/", "Holds 2 tasks.", "Usage page crew", "Docs refresh crew"],
  "needsyou-narrow-panel": ["Inbox 4", "Approval · High · hard to undo", "Run “npm publish --dry-run” in auth/", "Claude Code · Sonnet 5.5", "Waiting holds 1 task and 1 lease", "Conflict · Medium", "Waiting holds 2 tasks", "Questions · Low", "3 questions from OpenCode", "Answer 3"],
  "needsyou-budget": ["How the inbox is ranked", "Crew puts what matters most at the top and batches what can wait, so the person is interrupted less and never surprised.", "1 · Risk", "2 · Reversibility", "3 · Cost of waiting", "Batching", "What you control", "High-risk items (irreversible, security, money or data loss) interrupt at once"],
  "needsyou-demoted": [...INBOX, "Autonomy 1", "Auth moved from Full to On: 2 reverts in 14 days. Review or override.", "Autonomy by area · circuit breaker · 9 merges in the window", "Nothing is blocked. A High risk change in auth/ now waits for you before it merges; Low and Medium still merge by themselves.", "Review", "Override…"],
  "needsyou-stop": ["Stopped · too many denials", "Stopped · possible injected instructions", "It was denied 3 times in a row, so Crew stopped it. Nothing was lost; its worktree is intact.", "It read text that looks like instructions to the agent, so Crew stopped it.", "Open"],
  "needsyou-background": ["Paused: budget reached", "Needs approval", "Background · Fix failing CI", "Reviews 2", "Open"],
  "needsyou-source-error": ["Can’t read stops. The rest is shown.", "Try again", "Approve: run “npm publish --dry-run” in auth/", "Two workers disagree about the redirect rule"],
};

describeFrames("Needs you frames (Crew Designs, group 9)", DRAWN, SHOWS);

describe("the frames of the group", () => {
  it("names the two it does not build, with nothing drawn for them", () => {
    expect(NEEDSYOU_NOT_BUILT).toEqual(["needsyou-batch", "needsyou-conflict"]);
    for (const name of NEEDSYOU_NOT_BUILT) expect(NEEDSYOU_FRAMES as readonly string[]).not.toContain(name);
  });
});
