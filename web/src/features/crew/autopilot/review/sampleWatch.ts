import type { CommitRow, CommitState, DigestData, WatchData } from "./model";

/** Sample data for watch on main and the daily digest (Crew Designs v19). */

const commits: [string, string, string][] = [
  ["a7e19d2", "Fix a typo in the redirect error", "Merged 09:12"],
  ["5c2f7e0", "Rename a test helper", "Merged 10:22"],
  ["d03b8a1", "Update the Planner API example", "Merged 11:05"],
  ["9be4410", "Rename the legacy returnTo helper", "Merged 12:10"],
  ["c7f20b9", "Use next in the auth redirect", "Merged 14:32"],
  ["f1a0c63", "Add a rate limit to the login route", "Merged 14:50"],
];
const rows = (states: CommitState[]): CommitRow[] =>
  commits.map(([sha, title, merged], at) => ({ sha, title, merged, state: states[at] ?? "untested" }));

const FAILED = "Main failed at 15:11 · watched since 09:00";
const HEALTHY = "Main is healthy · watched since 09:00";
const SINCE = (summary: string) => ({ heading: "Merges since the last green main", summary });

export const watchBroke: WatchData = {
  health: FAILED,
  strip: {
    tone: "bad",
    heading: "Main broke at 15:11.",
    text: "CI failed on f1a0c63: auth/redirect.test.ts expects “/” but got “/home”. Crew is finding the merge that caused it.",
    actions: [{ label: "Open the failing run" }],
  },
  list: { ...SINCE("6 candidates · oldest at the top"), rows: rows([]) },
};

export const watchBisecting: WatchData = {
  health: FAILED,
  strip: {
    tone: "bad",
    text: "Bisecting: tested 2 of at most 3 steps. The cause is between 5c2f7e0 and 9be4410.",
    actions: [{ label: "Open the failing run" }],
  },
  list: {
    ...SINCE("6 candidates · oldest at the top"),
    rows: rows(["passes", "passes", "testing", "fails", "not-needed", "not-needed"]),
  },
};

export const watchReverted: WatchData = {
  health: HEALTHY,
  strip: {
    tone: "ok",
    heading: "Main is healthy again at 15:34.",
    text: "Crew reverted “Rename the legacy returnTo helper” because the failing check fails with it and passes without it.",
    actions: [{ label: "Open the revert" }, { label: "Open in Review", primary: true }],
  },
  evidence: {
    heading: "Why the cause was certain",
    summary: "decided by code",
    rows: [
      {
        text: "With 9be4410, redirect.test.ts fails.",
        evidence: "npm test auth/redirect · exit 1 · on the queue’s state at 9be4410 · reproduced twice (15:20 and 15:24)",
        ok: false,
      },
      { text: "Without 9be4410, it passes.", evidence: "npm test auth/redirect · exit 0 · on the revert candidate", ok: true },
      { text: "The revert passes every required check.", evidence: "5 of 5 checks on the combined state · merged 15:33", ok: true },
    ],
  },
  after: {
    heading: "What happens to the original work",
    text: "The task goes back to Build with a fix brief: “Rename returnTo without breaking the legacy login link.” You didn’t have to do anything, and the reverted merge shows in your Review queue.",
  },
};

export const watchAsks: WatchData = {
  health: FAILED,
  strip: {
    tone: "bad",
    heading: "Main broke at 15:11. Crew isn’t certain why, so it is asking you.",
    text: "The check fails with both candidates and passes with neither alone.",
    actions: [],
  },
  ask: {
    heading: "Needs you.",
    text: "Pick what to do. Nothing has been reverted.",
    actions: [{ label: "Revert the later one", primary: true }, { label: "Revert both" }, { label: "Re-run the check" }, { label: "Leave main" }],
  },
  evidence: {
    heading: "What Crew knows",
    summary: "3 runs",
    rows: [
      { text: "With 9be4410 and f1a0c63 together, the check fails.", evidence: "npm test auth · exit 1 · 15:20", ok: false },
      { text: "With 9be4410 alone, it passes.", evidence: "npm test auth · exit 0 · 15:24", ok: true },
      {
        text: "With f1a0c63 alone, it passes.",
        evidence: "npm test auth · exit 0 · 15:28",
        note: "So neither merge is the cause alone; their combination is.",
        ok: true,
      },
    ],
  },
};

export const watchFlaky: WatchData = {
  health: FAILED,
  strip: {
    tone: "bad",
    heading: "Main broke at 15:11. The failure didn’t reproduce, so Crew isn’t certain it was the merge.",
    text: "",
    actions: [],
  },
  ask: {
    heading: "Needs you.",
    text: "Nothing has been reverted. Main is as it was.",
    actions: [{ label: "Re-run the check", primary: true }, { label: "Revert 9be4410" }, { label: "Leave main" }],
  },
  evidence: {
    heading: "What Crew saw",
    summary: "3 runs",
    rows: [
      { text: "With 9be4410, the check failed once.", evidence: "npm test auth/redirect · exit 1 · 15:20", ok: false },
      { text: "With 9be4410, it passed on the re-run.", evidence: "npm test auth/redirect · exit 0 · 15:24", ok: true },
      {
        text: "",
        evidence: "npm test auth/redirect · exit 0 · 15:28",
        note: "A failure that doesn’t reproduce could be a flaky test, so it is not treated as certain.",
        ok: true,
      },
    ],
  },
};

export const digestDaily: DigestData = {
  date: "Sat 7 Nov 2026",
  groups: [
    {
      count: 9,
      heading: "Merged",
      summary: "1 High · 5 Medium · 3 Low · 3 h 40 min of agent time",
      rows: [
        { tier: "high", title: "Rotate the signing key", at: "15:02" },
        { tier: "medium", title: "Use next in the auth redirect", at: "14:32" },
        { tier: "low", title: "Update the login docs", at: "14:12" },
      ],
    },
    {
      count: 7,
      heading: "Waiting for your review",
      summary: "0 overdue · the oldest is due Sat 7 Nov, 19:02",
      rows: [
        { tier: "high", title: "Rotate the signing key", at: "top" },
        { tier: "medium", title: "Use next in the auth redirect", at: "22 h" },
        { tier: "low", title: "4 docs and tests", at: "batch" },
      ],
    },
    {
      count: 1,
      heading: "Reverted",
      summary: "Reverted automatically at 15:34 · cause certain",
      rows: [{ tier: "medium", title: "Rename the legacy returnTo helper", at: "15:34" }],
    },
  ],
  also: {
    heading: "Also today",
    text: "Auth moved from Full to On at 15:36: 2 reverts in 14 days, in 9 merges (Settings › Crew › Autopilot by area). 1 request that needs a person was denied (publishing the package), 0 trip wires, main broke once and was healthy again after 23 minutes. Costs are per engine, each in its own unit: Claude Code 412k tokens, Codex 62% of its weekly limit.",
  },
};

export const digestQuiet: DigestData = {
  date: "Sun 8 Nov 2026",
  groups: [],
  quiet: {
    heading: "Nothing merged today",
    text: "7 changes from Saturday are still waiting for your review. Crew was idle for most of the day.",
    action: "Open the Review queue",
  },
};
