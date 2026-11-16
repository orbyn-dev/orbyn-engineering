import { describeFrames } from "../shared/test-support";
import { ESCAPE_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "escape-log": ["As of Sat 7 Nov 2026, 17:00", "Legacy returnTo links still redirect", "A repo rule · runs in CI", "Shown from 10 merges, with the merges under it. Below that Crew shows the escapes themselves"],
  "escape-possible": ["1 later fix may be an escape.", "Rename the legacy returnTo helper", "The legacy login route lost its rate limit", "Added Tue 3 Nov, 17:05"],
  "escape-create-check": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl", "Claude Code · Sonnet 5.5 · auth/", "It must fail on 8d41b7c and pass on the revert. If it doesn’t, Crew doesn’t add it and asks you"],
  "escape-create-check-tooling": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts", "Claude Code · Sonnet 5.5 · auth/", "It must fail on c7f20b9 (the route is missing) and pass after “Add a rate limit to the login route”"],
  "escape-report": ["Main is healthy · watched since 09:00", "Use next in the auth redirect", "Claude Code · Sonnet 5.5 · auth/", "Add a rate limit to the login route · merged 16:10"],
  "escape-detail": ["Rename the legacy returnTo helper", "It fails on 8d41b7c and passes on the revert, which is required before it is added.", "Claude Code · Sonnet 5.5", "Open Autopilot by area"],
  "escape-detail-tooling": ["Use next in the auth redirect", "Classified: tooling gap", "Proof before it is added", "2 escapes in 9 merges · too few for a rate"],
  "escape-check-invalid": ["Move session helpers to auth/session.ts", "People were asked to sign in again after a session refresh. The revert passed its checks.", "The check passes on the bad commit", "tests/auth/session.test.ts"],
  "escape-dashboard": ["Main is healthy · watched since 09:00", "Claude Code · Sonnet 5.5", "OpenCode · DeepSeek V4", "Why some rows have no percentage"],
  "escape-route-counts": ["Add a keyboard shortcut to the web client", "The task, as Crew read it", "Claude Code · Sonnet 5.5", "Workers run as you, with your machine’s permissions."],
  "escape-route-shifted": ["Add pagination to the list endpoint", "The task, as Crew read it", "1 escape in 14 merges · 7%", "Workers run as you, with your machine’s permissions."],
  "escape-settings": ["Ask me each time · default", "10 merges · 2× · Proposed", "An escape not reverted", "Workers run as you, with your machine’s permissions."],
  "escape-empty": ["No escapes in the last 60 days"],
  "escape-low-sample": ["merges got past review and checks", "Merges per engine · last 60 days", "The old feed token still worked after it was replaced", "Claude Code · Sonnet 5.5"],
  "escape-error": ["Couldn’t read the run history on this device. Nothing was changed, and no escape was counted or removed.", "Not shown while the history can’t be read"],
  "escape-narrow-panel": ["Escaped in the last 60 days: 2 reverted, 1 found after the merge. 2 checks added, 1 being written.", "Check: Added Sat 7 Nov, 16:22", "Move session helpers to auth/session.ts", "Counts against Codex · GPT-5.5 · auth/"],
  "escape-narrow-window": ["got past review and checks", "Check: Added Sat 7 Nov, 16:22", "Sat 7 Nov, 15:02 · merged 14:32", "Counts against Codex · GPT-5.5 · auth/"],
};

describeFrames("Escape log frames (Crew Designs v20)", ESCAPE_FRAMES, SHOWS);
