import { describeFrames } from "../shared/test-support";
import { ESCAPE_LIVE_FRAMES } from "./live/screens";
import { ESCAPE_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "escape-create-check": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl", "Claude Code · Sonnet 5.5 · auth/", "It must fail on 8d41b7c and pass on the revert. If it doesn’t, Crew doesn’t add it and asks you"],
  "escape-create-check-tooling": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts", "Claude Code · Sonnet 5.5 · auth/", "It must fail on c7f20b9 (the route is missing) and pass after “Add a rate limit to the login route”"],
  "escape-report": ["Main is healthy · watched since 09:00", "Use next in the auth redirect", "Claude Code · Sonnet 5.5 · auth/", "Add a rate limit to the login route · merged 16:10"],
  "escape-dashboard": ["Main is healthy · watched since 09:00", "Claude Code · Sonnet 5.5", "OpenCode · DeepSeek V4", "Why some rows have no percentage"],
  "escape-route-counts": ["Add a keyboard shortcut to the web client", "The task, as Crew read it", "Claude Code · Sonnet 5.5", "Workers run as you, with your machine’s permissions."],
  "escape-route-shifted": ["Add pagination to the list endpoint", "The task, as Crew read it", "1 escape in 14 merges · 7%", "Workers run as you, with your machine’s permissions."],
  "escape-settings": ["Ask me each time · default", "10 merges · 2× · Proposed", "An escape not reverted", "Workers run as you, with your machine’s permissions."],
  "escape-narrow-panel": ["Escaped in the last 60 days: 2 reverted, 1 found after the merge. 2 checks added, 1 being written.", "Check: Added Sat 7 Nov, 16:22", "Move session helpers to auth/session.ts", "Counts against Codex · GPT-5.5 · auth/"],
  "escape-narrow-window": ["got past review and checks", "Check: Added Sat 7 Nov, 16:22", "Sat 7 Nov, 15:02 · merged 14:32", "Counts against Codex · GPT-5.5 · auth/"],
};

/** The frames still on drawn sample data: the log, its states, the possible list and the escape pages are the live
 * screens of Crew Designs v79, on the real routes, tested beside their own module (./live/live.test.tsx). */
const DRAWN: readonly string[] = ESCAPE_FRAMES.filter((name) => !(ESCAPE_LIVE_FRAMES as readonly string[]).includes(name));

describeFrames("Escape frames still drawn (Crew Designs v20)", DRAWN, SHOWS);
