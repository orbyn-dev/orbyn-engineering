// Every string the Autopilot screens show, as the Crew designs (v19) write it.
import type { HardKind, Mode, ResultKind, TemplateId, TriggerId } from "./model";

export const MODE_COPY: Record<Mode, { name: string; text: string }> = {
  off: { name: "Off", text: "Crew proves the work and stops. Agents’ requests come to you, and every change waits for you before it merges." },
  on: { name: "On", text: "The lead answers agents’ requests inside the hard limits. Low and Medium risk merge by themselves; High risk waits for you before merging." },
  full: { name: "Full", text: "The lead answers agents’ requests. Everything that passes merges, and High risk goes to the top of your Review queue. You review after the merge." },
};

export const MODE_LINE: Record<Mode, string> = {
  full: "Autopilot Full · the lead decides, inside limits set in code",
  on: "Autopilot On · the lead decides, inside limits set in code",
  off: "Autopilot Off · every request comes to you",
};

export const GUARD_LINE: Record<Mode, string> = {
  full: "Autopilot Full · the guard holds guarded commands for you",
  on: "Autopilot On · the guard holds guarded commands for you",
  off: "Autopilot Off · the guard holds guarded commands for you",
};

/** The always-denied list, by the core's rule id, in the order the designs show it. */
export const HARD_DENIED: [id: string, text: string][] = [
  ["N2", "Reading or sending secrets and credentials"],
  ["N3", "Network to hosts the repo hasn’t allowed"],
  ["N4", "Writing or deleting outside the worktree"],
  ["N5", "Force-push or history rewrite on main"],
  ["N6", "Changing branch protection, permissions or settings"],
  ["N7", "Editing answer keys, check files or .orbyn/crew.json"],
  ["N8", "Production deploys"],
  ["N9", "Publishing or messaging outside the repo"],
  ["N1", "Changing account, device or sign-in state"],
  ["N10", "Payments"],
];

export const HARD_FOOT =
  "Checked by Orbyn’s code before any model sees the request, so an instruction hidden in a page or a file does not change them. Read-only: a person can’t switch one off here, and nothing the lead decides can either. Always allowed: reads and writes inside a worker’s leased paths, and running the brief’s checks.";

export const RESULT: Record<ResultKind, { label: string; tone: "ok" | "no" | "am" | "dim" }> = {
  allowed: { label: "Allowed", tone: "ok" },
  denied_rule: { label: "Denied by rule", tone: "no" },
  denied_lead: { label: "Denied by lead", tone: "no" },
  keys_disagree: { label: "Keys disagree · denied", tone: "no" },
  person_only: { label: "Denied · for your review", tone: "am" },
  no_second_key: { label: "Denied · no second key", tone: "no" },
  held_you: { label: "Held for you", tone: "am" },
  held_expired: { label: "Expired", tone: "dim" },
  denied_you: { label: "Denied by you", tone: "no" },
  denied_held: { label: "Denied", tone: "no" },
  held_second_key: { label: "Held for a second key", tone: "dim" },
  approved_you: { label: "Approved by you", tone: "ok" },
  denied_second_key: { label: "Denied by the second key", tone: "no" },
  approved_two_keys: { label: "Approved by two keys", tone: "ok" },
};

export const HARD_CELL: Record<HardKind, { label: string; tone: "ok" | "no" | "am" | "dim" | "plain" }> = {
  always_allowed: { label: "Always allowed", tone: "ok" },
  passed: { label: "Passed", tone: "ok" },
  denied: { label: "Denied", tone: "no" },
  person_only: { label: "Person-only", tone: "am" },
  guard: { label: "Guard", tone: "am" },
  guard_two_keys: { label: "Guard · 2 keys", tone: "dim" },
};

export const LEGEND: ResultKind[] = ["allowed", "denied_rule", "denied_lead", "keys_disagree", "person_only"];

export const LEDGER_NOTE = "Every decision is in the ledger with its reason.";
export const GUARD_WORKERS = "Workers run as you, with your machine’s permissions.";

export const TEMPLATES: { id: TemplateId; name: string; short: string; starts: string; does: string; trigger: TriggerId }[] = [
  { id: "fix_ci", name: "Fix failing CI", short: "fix failing CI", starts: "CI fails on main", does: "Finds the failing check, fixes it in its own worktree and proves the fix on the queue’s state.", trigger: "ci_fails" },
  { id: "flaky_test", name: "Flaky test", short: "flaky test", starts: "A test fails and passes on the same commit", does: "Reruns it, finds the cause and fixes or quarantines it, with the evidence.", trigger: "schedule" },
  { id: "dependency_bump", name: "Dependency bump", short: "dependency bump", starts: "A dependency update opens", does: "Updates it, runs the repo’s checks and reads the changelog for breaking changes.", trigger: "dependency_update" },
  { id: "security_advisory", name: "Security advisory", short: "security advisory", starts: "An advisory is published for a dependency", does: "Checks whether the repo is affected, and bumps or patches it. It is High risk by default.", trigger: "dependency_update" },
];

export const TRIGGERS: { id: TriggerId; name: string; repeats: string }[] = [
  { id: "ci_fails", name: "CI fails on main", repeats: "One run per failing commit; a new failure during a run is added to it." },
  { id: "issue_label", name: "An issue gets a label", repeats: "The issue’s text is the brief. It is data, never instructions, and goes through the same checks." },
  { id: "schedule", name: "On a schedule", repeats: "Skipped if the previous run is still going." },
  { id: "dependency_update", name: "A dependency update opens", repeats: "One run per update; updates to the same package are grouped." },
];

/** The held-command card, as the Crew designs (v34) write it. Approving once is the desktop app's own dialog: its words are the shell's. */
export const HELD = {
  restart: "Held requests are cleared if Orbyn restarts.",
  expired: "This request expired. Nothing ran.",
  decided: "Already decided.",
  desktopOnly: "Approving once needs the Orbyn desktop app.",
  desktopOnlyNote: "Deny and Dismiss still work here.",
  refused: "Orbyn couldn’t confirm it was you. Try again from the desktop app.",
  refusedNote: "Nothing ran. The command is still held.",
  refusedNoteOther: "Nothing ran.",
  remote: "Do this from the computer the app runs on.",
  why: "Nothing runs until you approve it.",
} as const;
