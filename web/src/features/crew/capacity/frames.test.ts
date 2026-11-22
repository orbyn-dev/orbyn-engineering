import { describeFrames, describeInboxLabel } from "../shared/test-support";
import { CAPACITY_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "capacity-settings": ["Crew › Review capacity", "Change the limit", "Planned", "Pause new Low-priority starts", "Pauses new Low-priority starts while the queue is over the limit, and says so on the Crew page", "Workers run as you, with your machine’s permissions."],
  "capacity-settings-unset": ["Crew › Review capacity", "How many changes you can review", "Set a limit", "Review every batch · default", "Workers run as you, with your machine’s permissions."],
  "capacity-over": ["Main is healthy · watched since 09:00", "Revoke the old API tokens", "Add a rate limit to the login route", "Review by Sun 8 Nov · between 09:40 and 13:55"],
  "capacity-resumed": ["Main is healthy · watched since 09:00", "Review within 4 h · by 19:02 · 2 h left. High risk stays at the top, with an overdue alert.", "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts", "Review by Sun 8 Nov · between 09:40 and 11:20"],
  "capacity-paused-board": ["New low-priority starts are paused: your review queue is over its limit.", "Low priority · cleanup", "Rename returnTo to next in the session code", "Which target wins when both next and returnTo are set?"],
  "capacity-paused-card": ["Update the login docs for the new redirect rule", "Rename returnTo to next in the session code", "Which target wins when both next and returnTo are set?", "3 tasks, none affected"],
  "capacity-sampling": ["Batch review · 12 low-risk changes", "tests/auth/session.test.ts · merged 12:30 · Checks 4 of 4", "tests/helpers.ts · merged 10:22 · Checks 3 of 3", "tests/feed.test.ts · merged Fri 6 Nov, 15:20 · Checks 3 of 3"],
  "capacity-digest": ["Main is healthy · watched since 09:00", "Rotate the signing key", "0 overdue · the oldest is due Sat 7 Nov, 19:02 · limit 8", "Low-risk changes you didn’t have to review · all checks passed"],
  "capacity-coverage": ["Since provenance began", "14 of 38 merges aren’t in the map.", "lines no person has reviewed · 70%", "claude · Sonnet 5.5", "Open an area for its files", "Lines of a change that is neither reviewed nor waiting in your queue"],
  "capacity-coverage-area": ["As of Sat 7 Nov 2026, 17:00 · orbyn-engineering · 1,420 agent-written lines from 9 merges", "Use next in the auth redirect · Claude Code · Sonnet 5.5", "Move session helpers · Codex · GPT-5.5", "9 merges since Mon 12 Oct 2026"],
  "capacity-coverage-gaps": ["14 of 38 merges aren’t in the map.", "Try the 2 again", "Merged Sat 31 Oct, 10:20 · codex · GPT-5.5 · web/app", "Update the login docs for returnTo", "12 merges · before Mon 12 Oct 2026"],
  "capacity-coverage-empty": ["Since provenance began", "No agent-written lines are recorded yet", "Open the Review queue"],
  "capacity-coverage-low": ["Since provenance began", "2 of 5 merges aren’t in the map.", "lines no person has reviewed · 63%"],
  "capacity-coverage-error": ["Since provenance began", "Couldn’t read the provenance notes on this device. Nothing was changed, and no line was counted or removed.", "Can’t reach the Orbyn service.", "Not shown while the notes can’t be read"],
  "capacity-narrow-panel": ["New low-priority starts are paused", "Checks 5 of 5 · Not verified: 2", "Revoke the old API tokens", "7 more Medium changes. Open in Crew shows the whole queue."],
  "capacity-narrow-window": ["New low-priority starts are paused: your review queue is over its limit.", "Checks 5 of 5 · Not verified: 2", "Use next in the auth redirect", "6 more Medium changes follow in this list."],
  "capacity-coverage-narrow-panel": ["3,660 agent-written lines · 62% not read", "Reviewed 910 · In your queue 310 · Not in a queue 200", "Reviewed 120 · In your queue 0 · Not in a queue 940", "Reviewed 350 · In your queue 80 · Not in a queue 750"],
  "capacity-coverage-narrow-window": ["3,660 agent-written lines · 62% not read", "510 of 1,420 not read · 36%", "Reviewed 120 · In your queue 0 · Not in a queue 940", "Reviewed 350 · In your queue 80 · Not in a queue 750"],
};

describeFrames("Review capacity frames (Crew Designs v21, with the settings and coverage frames at v77)", CAPACITY_FRAMES, SHOWS);
describeInboxLabel(CAPACITY_FRAMES);
