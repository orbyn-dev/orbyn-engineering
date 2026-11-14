// Sample items for the drawn frames, copied from Crew Designs (needsyou-*). Samples only: nothing here is read from a route,
// and four of the kinds (denied, questions, conflict, autonomy) have no route behind them yet.
import type { NeedsItem } from "./model";

const item = (over: Partial<NeedsItem> & Pick<NeedsItem, "id" | "kind" | "title" | "actions">): NeedsItem => ({
  risk: null,
  undo: null,
  from: "",
  cost: null,
  since: null,
  ref: over.id,
  ...over,
});

const DENIED = item({
  id: "s-denied",
  kind: "denied",
  title: "A request only you can approve was denied: publish the package",
  from: "Claude Code · Sonnet 5.5 · person-only",
  engine: "Claude Code · Sonnet 5.5",
  cost: "The task went on without it. Nothing is blocked; do it yourself when you choose.",
  actions: [{ key: "open", label: "Open the request", primary: true }, { key: "dismiss", label: "Dismiss" }],
});
const CONFLICT = item({
  id: "s-conflict",
  kind: "conflict",
  title: "Two workers disagree about the redirect rule",
  from: "Codex · OpenCode · conflicting board writes",
  engine: "Codex · OpenCode",
  cost: "Waiting holds 2 tasks. The choice is yours; the evidence is side by side.",
  actions: [{ key: "decide", label: "Decide", primary: true }, { key: "open-both", label: "Open both" }],
});
const QUESTIONS = item({
  id: "s-questions",
  kind: "questions",
  title: "3 questions from OpenCode, batched",
  from: "OpenCode · DeepSeek V4 · questions",
  engine: "OpenCode · DeepSeek V4",
  cost: "Waiting holds 1 read-only task. Answer all three together.",
  actions: [{ key: "answer", label: "Answer 3", primary: true }, { key: "open-thread", label: "Open thread" }],
});
const MAIN = item({
  id: "s-main",
  kind: "main",
  title: "Main broke and the cause isn’t certain",
  from: "Watch on main · 9be4410 and f1a0c63 together",
  cost: "Nothing has been reverted. Main stays as it is until you choose.",
  actions: [{ key: "open", label: "Open", primary: true }, { key: "rerun", label: "Re-run the check" }],
});
const APPROVAL = item({
  id: "s-approval",
  kind: "approval",
  title: "Approve: run “npm publish --dry-run” in auth/",
  from: "Claude Code · Sonnet 5.5 · approval",
  engine: "Claude Code · Sonnet 5.5",
  cost: "Waiting holds 1 task and its lease on auth/redirect.ts. Nothing else is blocked.",
  actions: [{ key: "approve", label: "Approve", primary: true }, { key: "deny", label: "Deny" }],
});
const REVIEW = item({
  id: "s-review",
  kind: "review",
  title: "Review “Use next in the auth redirect”",
  risk: "medium",
  undo: "easy",
  from: "Autopilot is Off · every change waits for you",
  cost: "Waiting costs nothing now, and the change stays as it is.",
  actions: [{ key: "open-review", label: "Open review", primary: true }, { key: "later", label: "Later" }],
});
const AUTONOMY = item({
  id: "s-autonomy",
  kind: "autonomy",
  title: "Auth moved from Full to On: 2 reverts in 14 days. Review or override.",
  from: "Autonomy by area · circuit breaker · 9 merges in the window",
  cost: "Nothing is blocked. A High risk change in auth/ now waits for you before it merges; Low and Medium still merge by themselves.",
  actions: [{ key: "review", label: "Review", primary: true }, { key: "override", label: "Override…" }, { key: "dismiss", label: "Dismiss" }],
});

export const QUESTIONS_ITEM = QUESTIONS;
export const MAIN_ITEM = MAIN;

/** needsyou-inbox: Autopilot Full, the default. */
export const FULL_ITEMS: NeedsItem[] = [DENIED, CONFLICT, QUESTIONS, MAIN];
/** needsyou-inbox-off and needsyou-approval: Autopilot Off. */
export const OFF_ITEMS: NeedsItem[] = [APPROVAL, CONFLICT, QUESTIONS, REVIEW];
/** needsyou-demoted: the circuit breaker's line ranks first, below what waits on you in the frame's own order. */
export const DEMOTED_ITEMS: NeedsItem[] = [AUTONOMY, DENIED, CONFLICT, QUESTIONS];

const line = (id: string, kind: NeedsItem["kind"], title: string, from: string, cost: string): NeedsItem =>
  item({ id, kind, title, from, cost, ...(kind === "review" ? { risk: "medium", undo: "easy" } : {}), actions: [{ key: "open", label: "Open" }] });

const STOP_DENIALS = item({
  id: "s-stop-denials",
  kind: "stopped",
  title: "Stopped · too many denials",
  from: "Login redirect crew · Worker 2 · Claude Code · Sonnet 5.5",
  cost: "3 denials in a row stopped the task. Nothing else is blocked; the other tasks keep going.",
  actions: [{ key: "open", label: "Open", primary: true }],
});
const STOP_INJECTED = item({
  id: "s-stop-injected",
  kind: "stopped",
  title: "Stopped · possible injected instructions",
  from: "Login redirect crew · Worker 1 · Claude Code · Sonnet 5.5",
  cost: "A page the worker read contained text that looks like instructions. The task stopped before it ran anything the text asked for.",
  actions: [{ key: "open", label: "Open", primary: true }],
});
/** needsyou-stop: a task stopped by a trip wire, for each reason. */
export const STOP_ITEMS: NeedsItem[] = [STOP_DENIALS, STOP_INJECTED];

/** needsyou-background: a background run waiting on the person, paused for its budget or needing approval. */
export const BACKGROUND_ITEMS: NeedsItem[] = [
  item({ id: "s-bg-budget", kind: "background", title: "Paused: budget reached", from: "Background crew · Check dependencies weekly · Claude Code", cost: "The run stopped at its budget. Raise the budget, or leave it paused.", actions: [{ key: "open", label: "Open", primary: true }] }),
  item({ id: "s-bg-approval", kind: "background", title: "Needs approval", from: "Background crew · Update the docs index · Sonnet 5.5", cost: "A command is held for your approval before the run can go on.", actions: [{ key: "open", label: "Open", primary: true }] }),
];

/** needsyou-many: the top four of 27, one line each. */
export const MANY_ITEMS: NeedsItem[] = [
  line("m1", "questions", "Answer: which package manager?", "Claude Code · docs crew", "Holds 1 task."),
  line("m2", "review", "Review: licence file added", "Codex · deps crew", "Holds 1 task."),
  line("m3", "approval", "Approve: rm -rf build/", "Claude Code · usage crew", "Holds 2 tasks."),
  line("m4", "questions", "Answer: keep the old export name?", "OpenCode · usage crew", "Holds 1 task."),
];
export const MANY_GROUPS = [
  { name: "Usage page crew", count: 9 },
  { name: "Docs refresh crew", count: 14 },
];

/** needsyou-narrow-panel: three cards. */
export const NARROW_ITEMS: NeedsItem[] = [
  { ...APPROVAL, title: "Run “npm publish --dry-run” in auth/", from: "Claude Code · Sonnet 5.5", cost: "Waiting holds 1 task and 1 lease" },
  { ...CONFLICT, from: "Codex · OpenCode", cost: "Waiting holds 2 tasks", actions: [{ key: "decide", label: "Decide", primary: true }] },
  { ...QUESTIONS, title: "3 questions from OpenCode", from: "OpenCode · DeepSeek V4", cost: null, actions: [{ key: "answer", label: "Answer 3", primary: true }] },
];
