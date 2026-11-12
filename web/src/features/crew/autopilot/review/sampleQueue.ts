import type { BatchData, QueueData, QueueEntry } from "./model";

/** Sample data for the Review queue frames (Crew Designs v19). Display text, not computed. */

const HEALTHY = "Main is healthy · watched since 09:00";
const WINDOW_HIGH = "Review within 4 h · by 19:02 · 3 h left. High risk stays at the top, with an overdue alert.";

export const highKey: QueueEntry = {
  id: "keys-rotate",
  tier: "high",
  title: "Rotate the signing key",
  meta: "Merged by Crew at 15:02 · Claude Code · Opus 5.5 · crew/keys-rotate · keys/rotate.ts, auth/login.ts",
  proof: ["Checks 5 of 5", "Reviewed by 3 lanes · 2 vendors", "1 finding fixed · 0 open", "Not verified: 2", "156k tokens · 41 min"],
  state: "waiting",
  top: true,
  footer: WINDOW_HIGH,
};

export const mediumRedirect: QueueEntry = {
  id: "redirect-impl",
  tier: "medium",
  title: "Use next in the auth redirect",
  meta: "Merged by Crew at 14:32 · Claude Code · Sonnet 5.5 · crew/redirect-impl · auth/redirect.ts",
  proof: ["Checks 3 of 3", "Reviewed by 2 lanes · 2 vendors", "0 findings", "Not verified: 4", "52k tokens · 22 min"],
  state: "waiting",
  footer: "Review by Sun 8 Nov, 14:32 · 22 h left",
};

export const mediumLimit: QueueEntry = {
  id: "login-limit",
  tier: "medium",
  title: "Add a rate limit to the login route",
  meta: "Merged by Crew at 14:50 · Codex · GPT-5.5 · crew/login-limit · auth/login.ts",
  proof: ["Checks 4 of 4", "Reviewed by 2 lanes · 1 vendor", "0 findings", "Not verified: 1", "32k tokens · 14 min"],
  state: "waiting",
  footer: "Review by Sun 8 Nov, 14:50 · 22 h left",
};

export const lowBatch: QueueEntry = {
  id: "low-batch",
  tier: "low",
  title: "4 low-risk changes: docs and tests",
  meta: "Merged by Crew between 11:05 and 14:12 · docs/login.md, tests/",
  proof: ["All checks passed", "Not verified: 0 to 1 each", "50k tokens together"],
  state: "waiting",
  footer: "Review in a batch whenever you like",
  batchSize: 4,
};

export const reverted: QueueEntry = {
  id: "returnto-rename",
  tier: "medium",
  title: "Rename the legacy returnTo helper",
  meta: "Merged 12:10 · reverted by Crew at 15:34 · Claude Code · Sonnet 5.5",
  proof: ["Main broke at 15:11", "Revert checked on the combined state", "Cause was certain"],
  state: "reverted",
  footer: "Reverted automatically · the fix brief is queued",
};

export const queue: QueueData = { entries: [highKey, mediumRedirect, mediumLimit, lowBatch], health: HEALTHY, counts: { reverted: 1 } };

export const queueEmpty: QueueData = { entries: [], health: HEALTHY, counts: { reverted: 0 } };

export const queueOverdue: QueueData = {
  health: HEALTHY,
  counts: { "to-review": 7, high: 1, overdue: 1, reverted: 1 },
  banner: {
    tone: "amber",
    heading: "1 review is overdue.",
    text: "The review windows are 4 hours for High risk and 24 hours for Medium. You can change them in Settings › Crew.",
    actions: ["Settings"],
  },
  entries: [
    {
      id: "session-move",
      tier: "medium",
      title: "Move session helpers to auth/session.ts",
      meta: "Merged by Crew at Fri 6 Nov, 10:40 · Claude Code · Sonnet 5.5 · crew/session-move",
      proof: ["Checks 4 of 4", "Reviewed by 2 lanes · 1 vendor", "0 findings", "Not verified: 1", "32k tokens · 14 min"],
      state: "overdue",
      overdueTag: "Review overdue",
      footer: "It was due Sat 7 Nov, 10:40, 4 h ago. It stays here until you look.",
    },
    mediumRedirect,
  ],
};

export const queueOverdueHigh: QueueData = {
  health: HEALTHY,
  counts: { "to-review": 7, high: 1, overdue: 1, reverted: 1 },
  banner: {
    tone: "amber",
    heading: "1 High review is overdue.",
    text: "It was due at 19:02. An overdue alert was sent at 19:02.",
    actions: ["Settings"],
  },
  entries: [
    {
      ...highKey,
      state: "overdue",
      overdueTag: "Review overdue",
      footer: "Its 4-hour window ended at 19:02, 1 h ago. It stays at the top until you look.",
    },
    { ...mediumRedirect, footer: "Review by Sun 8 Nov, 14:32 · 19 h left" },
  ],
};

export const queueReverted: QueueData = {
  health: HEALTHY,
  counts: { "to-review": 7, high: 1, overdue: 0, reverted: 1 },
  entries: [reverted, mediumRedirect],
};

export const queueDemoted: QueueData = {
  health: HEALTHY,
  counts: { "to-review": 7, high: 1, overdue: 0, reverted: 1 },
  banner: {
    tone: "neutral",
    heading: "Auth moved from Full to On at 15:36.",
    text: "2 reverts in 14 days, in 9 merges. A High risk change in auth/ now waits for you before it merges; Low and Medium still merge by themselves.",
    actions: ["Open the area", "Override…"],
  },
  entries: [
    {
      ...mediumLimit,
      meta: "Merged by Crew at 16:10 · Codex · GPT-5.5 · crew/login-limit · auth/login.ts",
      marker: "Auth is on On: 2 reverts in 14 days",
      footer: "Review by Sun 8 Nov, 16:10 · 23 h left",
    },
    mediumRedirect,
    {
      id: "docs-login",
      tier: "low",
      title: "Update the login docs",
      meta: "Merged by Crew at 14:12 · Claude Code · Sonnet 5.5 · docs/login.md",
      proof: ["Checks 4 of 4", "0 findings", "Not verified: 1", "32k tokens · 14 min"],
      state: "waiting",
      footer: "Batch review · no deadline",
    },
  ],
};

export const queuePromoted: QueueData = {
  health: HEALTHY,
  counts: { high: 1, overdue: 0, reverted: 1 },
  banner: {
    tone: "neutral",
    heading: "web/ earned Full on Sat 31 Oct, 10:00.",
    text: "22 merges in 30 days with no revert and no escape. A High risk change in web/ now merges by itself and goes to the top of this queue.",
    actions: [],
  },
  entries: [
    {
      ...highKey,
      id: "web-session",
      title: "Replace the web client’s session storage",
      meta: "Merged by Crew at Sat 31 Oct, 10:42 · Claude Code · Opus 5.5 · crew/web-session · web/session.ts",
      marker: "Promoted: web is on Full",
      footer: "Review within 4 h · by 14:42 · 3 h left. High risk stays at the top, with an overdue alert.",
    },
    {
      ...mediumRedirect,
      id: "web-shortcut",
      title: "Add a keyboard shortcut to the web client",
      meta: "Merged by Crew at Sat 31 Oct, 10:20 · Codex · GPT-5.5 · crew/web-shortcut · web/shortcuts.ts",
      proof: ["Checks 3 of 3", "0 findings", "Not verified: 4", "52k tokens · 22 min"],
      footer: "Review by Sun 1 Nov, 10:20 · 23 h left",
    },
  ],
};

export const batch: BatchData = {
  heading: "Batch review · 12 low-risk changes",
  lead: { heading: "12 Low risk changes merged since Fri 6 Nov.", text: "Every one passed all its checks. 6 are shown." },
  group: "Docs and tests",
  groupSummary: "12 changes · 160k tokens together · all checks passed",
  total: 12,
  rows: [
    { title: "Update the login docs", detail: "docs/login.md · merged 14:12 · Checks 2 of 2", cost: "11k tokens", checked: true },
    { title: "Fix a typo in the redirect error", detail: "docs/errors.md · merged 13:48 · Checks 2 of 2", cost: "4k tokens", checked: true },
    { title: "Add a test for returnTo removal", detail: "tests/auth/session.test.ts · merged 12:30 · Checks 4 of 4", cost: "27k tokens", checked: true },
    { title: "Update the Planner API example", detail: "docs/api.md · merged 11:05 · Checks 2 of 2", cost: "8k tokens", checked: true },
    { title: "Rename a test helper", detail: "tests/helpers.ts · merged 10:22 · Checks 3 of 3", cost: "14k tokens", checked: true },
    { title: "Fix a broken link in the README", detail: "README.md · merged 09:12 · Checks 1 of 1", cost: "3k tokens", checked: true },
  ],
};
