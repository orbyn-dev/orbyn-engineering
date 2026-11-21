import { describeFrames } from "../shared/test-support";
import { ESCAPE_LIVE_FRAMES } from "./live/screens";
import { ESCAPE_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "escape-create-check": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl", "Claude Code · Sonnet 5.5 · auth/", "It must fail on 8d41b7c and pass on the revert. If it doesn’t, Crew doesn’t add it and asks you"],
  "escape-create-check-tooling": ["Main is healthy · watched since 09:00", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts", "Claude Code · Sonnet 5.5 · auth/", "It must fail on c7f20b9 (the route is missing) and pass after “Add a rate limit to the login route”"],
  "escape-report": ["Main is healthy · watched since 09:00", "Use next in the auth redirect", "claude · web/auth", "There is no undo in v1."],
  "escape-dashboard": ["Main is healthy · watched since 09:00", "claude · Sonnet 5.5", "opencode · DeepSeek V4", "Why there is no percentage", "Not measured"],
  "escape-route-counts": ["Add a keyboard shortcut to the web client", "The task, as Crew read it", "Escape counts in web/app · last 60 days", "Not measured", "Workers run as you, with your machine’s permissions."],
  "escape-route-shifted": ["Add pagination to the list endpoint", "The task, as Crew read it", "Escape counts in web/api · last 60 days", "Both engines suit the task equally.", "Workers run as you, with your machine’s permissions."],
};

/** The frames still on drawn sample data: the log with its states and narrow variants, the possible list and the escape
 * pages are the live screens of Crew Designs v79, on the real routes, tested beside their own module (./live/live.test.tsx). */
const DRAWN: readonly string[] = ESCAPE_FRAMES.filter((name) => !(ESCAPE_LIVE_FRAMES as readonly string[]).includes(name));

describeFrames("Escape frames still drawn (Crew Designs v20 and v79)", DRAWN, SHOWS);
