import type { BoardColumn, ForecastData, MergeItem, MergeQueueData } from "./model";

/** Sample data for the opened merge, the Merge queue and the Taskboard tiers (Crew Designs v19). */

const diff: MergeItem["diff"] = [
  { kind: "del", text: "- if (next) return redirect(next)" },
  { kind: "add", text: "+ if (next && isInsideApp(next)) return redirect(next)" },
  { kind: "add", text: '+ return redirect("/")' },
];

export const mergeItem: MergeItem = {
  title: "Use next in the auth redirect",
  tier: "medium",
  merged: "Merged 14:32",
  summary: "Login now returns people to the page they came from when next points inside the app, and to / otherwise.",
  builtBy: "Claude Code · Sonnet 5.5 · crew/redirect-impl",
  mergedAt: "Sat 7 Nov 2026, 14:32 · commit c7f20b9 on main",
  spent: "22 min · 52k tokens",
  reviewBy: "Sun 8 Nov, 14:32 · 22 h left",
  diff,
  claims: [
    { text: "Paths outside the app are never followed.", evidence: "npm test auth · exit 0 · c7f20b9", ok: true },
    { text: "A next of //evil.com goes to /.", evidence: "npm test auth/redirect · exit 0 · c7f20b9", ok: true },
    { text: "The change touches just auth/redirect.ts.", evidence: "File scope · exit 0", ok: true },
  ],
  notVerified: [
    "Other callers of redirect() outside auth/",
    "Safari and Firefox behaviour",
    "That it doesn’t slow login",
    "The mobile app’s use of next",
  ],
  reviewedBy: [
    { lane: "Correctness", who: "Codex · OpenAI · 0 findings" },
    { lane: "Performance", who: "OpenCode · DeepSeek · 0 findings" },
  ],
  dependents: [],
  revert: {
    commit: "c7f20b9 · Use next in the auth redirect",
    later: "1 (Add a rate limit to the login route): Crew checks it still works without this change",
    spent: "About 17k tokens · 6 min",
    together: "",
    reason: "It breaks the legacy login link.",
  },
  commentBrief: {
    prompt: "The redirect should also reject paths that start with a backslash, and the test should say so.",
    draft:
      "“Reject a next that starts with / followed by a backslash. Add a test. Keep the change inside auth/redirect.ts.” You can edit it before it is sent.",
  },
};

export const mergeItemWithDependents: MergeItem = {
  ...mergeItem,
  dependents: [
    {
      commit: "f1a0c63 · Add a rate limit to the login route",
      title: "Add a rate limit to the login route",
      note: "“Add a rate limit to the login route” (14:50) calls the new check this merge added. Reverting this one alone would break it, so Crew asks you before it reverts anything, even under Full.",
    },
  ],
  revert: { ...mergeItem.revert, together: "2 merges · Crew checks the combined revert on the queue’s state · about 30k tokens · 9 min" },
};

export const mergeQueue: MergeQueueData = {
  heading: "Merge queue",
  summary: "3 candidates · 1 merged today",
  rows: [
    { n: 1, title: "Rotate the signing key", detail: "crew/keys-rotate · Low conflict risk · checks passed on its own branch", state: "merged", status: "Merged 15:02", tier: "high" },
    { n: 2, title: "Use next in the auth redirect", detail: "Rebased on 1 · re-running 2 of 3 checks on the combined state", state: "running", status: "Re-checking combined", tier: "medium" },
    { n: 3, title: "Update the login docs", detail: "Waiting for 2 · will be rebased on the result", state: "waiting", status: "Waiting", tier: "low" },
  ],
  notes: [
    {
      heading: "How the queue works",
      items: [
        "Verified work joins the queue in order.",
        "Each candidate is rebased on the queue ahead of it, and its required checks run again on the combined state before main moves.",
        "If the combined checks fail, the candidate leaves the queue with its reason and goes back to Build.",
      ],
    },
    {
      heading: "Merging is code",
      text: "The queue never merges on a model’s say-so. It merges when the required checks pass on the combined state, and not otherwise.",
    },
  ],
};

export const mergeQueueRemoved: MergeQueueData = {
  heading: "Merge queue",
  summary: "2 candidates · 1 removed",
  rows: [
    { n: 1, title: "Rotate the signing key", detail: "Merged", state: "merged", status: "Merged 15:02", tier: "high" },
    {
      n: 2,
      title: "Use next in the auth redirect",
      detail: "Removed: its checks failed on the combined state (exit 1): redirect.test.ts expects / but the queue’s change returns /home",
      state: "removed",
      status: "Left the queue",
      tier: "medium",
    },
    { n: 3, title: "Update the login docs", detail: "Re-checking without 2", state: "running", status: "Re-checking combined", tier: "low" },
  ],
  notes: [
    {
      heading: "What happens next",
      text: "“Use next in the auth redirect” goes back to Build with the failing command, the output and the commit. The queue carries on without it.",
    },
  ],
};

export const forecast: ForecastData = {
  headline: "Conflict forecast",
  conflict:
    "“Move session helpers to auth/session.ts” and “Rename the legacy returnTo helper” both touch auth/session.ts.",
  facts: [
    { label: "Forecast", value: "A merge conflict is likely if they run side by side" },
    { label: "From", value: "The dependency graph: both import session.ts" },
    { label: "Router’s plan", value: "Run them in order, the rename, then the move" },
  ],
  order: [
    { n: 1, title: "Rename the legacy returnTo helper", detail: "Claude Code · starts now", status: "Running", running: true },
    { n: 2, title: "Move session helpers to auth/session.ts", detail: "Claude Code · starts after 1 merge", status: "Waits for 1", running: false },
  ],
  why: {
    heading: "Why ordered, not parallel",
    text: "Side by side, both would need the same file, and agents working on one file collide. Ordering costs a little time and avoids a rework round.",
  },
};

export const board: BoardColumn[] = [
  {
    label: "Low · Full",
    tone: "ok",
    cards: [{ title: "Update the login docs", engine: "Codex", model: "GPT-5.5 · Low", checks: "2/2 checks", paths: "docs/login.md", tier: "low", line: "Merged by itself · 14:12" }],
  },
  {
    label: "Medium · Full",
    tone: "ok",
    cards: [{ title: "Use next in the auth redirect", engine: "Claude Code", model: "Sonnet 5.5 · Medium", checks: "3/3 checks", paths: "auth/redirect.ts", tier: "medium", line: "Merged by itself · 14:32" }],
  },
  {
    label: "High · Full",
    tone: "ok",
    cards: [{ title: "Rotate the signing key", engine: "Claude Code", model: "Opus 5.5 · High", checks: "5/5 checks", paths: "keys/rotate.ts", tier: "high", line: "Merged · top of your queue" }],
  },
  {
    label: "High · On",
    tone: "warn",
    cards: [{ title: "Rotate the signing key", engine: "Claude Code", model: "Opus 5.5 · High", checks: "5/5 checks", paths: "keys/rotate.ts", tier: "high", line: "Waiting for you before the merge (On)", needs: true }],
  },
];
