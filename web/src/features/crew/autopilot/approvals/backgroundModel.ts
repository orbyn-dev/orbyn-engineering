// What the Crew API says about background crews, as the words and rows the Background screens draw (Crew designs: background-list,
// background-runs, background-triggers). Pure: the same reads give the same rows. The wire names and the design's template and trigger
// ids differ, so the two maps below are the only place that knows both.
import type { BackgroundCrew as WireCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun as WireRun } from "../../../../generated/BackgroundRun";
import type { BackgroundTemplate } from "../../../../generated/BackgroundTemplate";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import type { BackgroundTriggerKind } from "../../../../generated/BackgroundTriggerKind";
import type { BackgroundBudget } from "../../../../generated/BackgroundBudget";
import type { BackgroundDailyBudget } from "../../../../generated/BackgroundDailyBudget";
import type { Mode, TemplateId, TriggerId } from "./model";

// Background crews are kept on this device, in the service's data folder. The route serves the crews, their runs and the on/off
// switch; no trigger fires yet, so the screens say so wherever the designs do.
export interface BackgroundCrew {
  id: string;
  name: string;
  template: TemplateId;
  startsWhen: string;
  lastRun: string | null;
  enabled: boolean;
  /** Where the crew stands, as the route reports it: "Idle", "Running", "Stopped · Budget" or "Needs review". Absent in the preview views. */
  status?: string;
  /** The crew's own Autopilot setting. The route does not report one, so it is null on the live page and the column is not drawn. */
  mode: Mode | null;
}
export interface BackgroundRun {
  id: string;
  name: string;
  template: TemplateId;
  startedBy: string;
  result: string;
  /** The route's three run states. "Merged" and "Failed" are only drawn by the preview views: the route reports neither. */
  state: "Merged" | "Running" | "Stopped" | "Failed" | "Needs review" | null;
  /** Why a stopped run stopped: "Budget", "Repeated failure", "Engine error" or "Turned off". */
  reason?: string | null;
  mode: Mode | null;
}
export interface BackgroundView {
  crews: BackgroundCrew[];
  /** How many crews the route holds, when more than `crews` were read. */
  total?: number;
  runs: BackgroundRun[];
  /** What each trigger is set to across the crews, for the Triggers table; absent means "Not set". */
  configured?: Partial<Record<TriggerId, string>>;
}


export const TEMPLATE_OF: Record<BackgroundTemplate, TemplateId> = {
  fix_failing_ci: "fix_ci",
  flaky_test: "flaky_test",
  dependency_bump: "dependency_bump",
  security_advisory: "security_advisory",
};
export const WIRE_TEMPLATE: Record<TemplateId, BackgroundTemplate> = {
  fix_ci: "fix_failing_ci",
  flaky_test: "flaky_test",
  dependency_bump: "dependency_bump",
  security_advisory: "security_advisory",
};
export const TRIGGER_OF: Record<BackgroundTriggerKind, TriggerId> = {
  ci_failure: "ci_fails",
  issue_label: "issue_label",
  schedule: "schedule",
  dependency_update: "dependency_update",
  watch: "watch",
};
export const WIRE_TRIGGER: Record<TriggerId, BackgroundTriggerKind> = {
  ci_fails: "ci_failure",
  issue_label: "issue_label",
  schedule: "schedule",
  dependency_update: "dependency_update",
  watch: "watch",
};

/** What a new crew starts with, and the route's limits: agent turns 1 to 1000 and minutes 1 to 1440 for each run, and for each
 *  engine a day of runs 1 to 1000 and turns 1 to 10000 that resets at the person's midnight. */
export const DEFAULT_BUDGET: BackgroundBudget = { turns: 40, minutes: 60 };
export const DEFAULT_DAILY: BackgroundDailyBudget = { runs: 2, turns: 80 };
export const LIMITS = { turns: 1000, minutes: 1440, dailyRuns: 1000, dailyTurns: 10_000, hours: 720, text: 100, name: 80 } as const;

const WEEKDAY = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number) => String(n).padStart(2, "0");
const clock = (at: Date) => `${pad(at.getHours())}:${pad(at.getMinutes())}`;
const day = (at: Date) => `${WEEKDAY[at.getDay()]} ${at.getDate()} ${MONTH[at.getMonth()]}`;
const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

/** "15:11" for today, "Fri 6 Nov" before. */
export function whenShort(iso: string, now: Date): string {
  const at = new Date(iso);
  return sameDay(at, now) ? clock(at) : day(at);
}
/** "Sat 7 Nov, 15:11". */
export const whenLong = (iso: string): string => {
  const at = new Date(iso);
  return `${day(at)}, ${clock(at)}`;
};

/** "When CI fails on main": the trigger of a crew, in words. */
export function startsWhen(crew: Pick<WireCrew, "trigger">): string {
  const { trigger } = crew;
  switch (trigger.kind) {
    case "ci_failure":
      return trigger.branch ? `When CI fails on ${trigger.branch}` : "When CI fails";
    case "issue_label":
      return `When an issue gets the label ${trigger.label ?? ""}`.trim();
    case "schedule":
      return trigger.every_hours === 1 ? "Every hour" : `Every ${trigger.every_hours ?? "?"} hours`;
    case "dependency_update":
      return "When a dependency update opens";
    case "watch":
      return "Watch";
  }
}

/** The Triggers table's "Configured as" for one trigger setting, or null when nothing is set. */
export function configuredAs(trigger: BackgroundTrigger): string | null {
  switch (trigger.kind) {
    case "ci_failure": {
      const line = [trigger.workflow, trigger.branch ? `on ${trigger.branch}` : null].filter(Boolean).join(" ");
      return line || null;
    }
    case "issue_label":
      return trigger.label ? `Label: ${trigger.label}` : null;
    case "schedule":
      return trigger.every_hours ? (trigger.every_hours === 1 ? "Every hour" : `Every ${trigger.every_hours} hours`) : null;
    case "dependency_update":
      return trigger.ecosystem ? `Dependabot or Renovate PRs · ${trigger.ecosystem}` : "Dependabot or Renovate PRs";
    case "watch":
      return null;
  }
}

const STOP_REASON: Record<NonNullable<WireRun["stop_reason"]>, string> = {
  budget: "Budget",
  repeated_failure: "Repeated failure",
  engine_error: "Engine error",
  disabled: "Turned off",
};
const reasonOf = (run: WireRun) => (run.stop_reason ? STOP_REASON[run.stop_reason] : null);

/** "12 turns, 9 min", or "running, 6 min" while it goes: what a run has spent, as the list draws it. */
const spent = (run: WireRun) => `${run.turns_spent} ${run.turns_spent === 1 ? "turn" : "turns"}, ${run.minutes_spent} min`;

/** "Sat 7 Nov, 15:11 · 12 turns, 9 min": the newest run of a crew, or "Never run". */
export function lastRunLine(run: WireRun | undefined): string | null {
  if (!run) return null;
  return `${whenLong(run.started_at)} · ${run.state === "running" ? `running, ${run.minutes_spent} min` : spent(run)}`;
}

/** A crew's state as the route reports it; a stopped crew says why, from its newest run. */
export function crewStatus(crew: WireCrew, newest: WireRun | undefined): string {
  if (crew.state === "running") return "Running";
  if (crew.state === "needs_review") return "Needs review";
  if (crew.state === "stopped") return `Stopped${newest && reasonOf(newest) ? ` · ${reasonOf(newest)}` : ""}`;
  return "Idle";
}

const STARTED_BY: Record<BackgroundTriggerKind, (trigger: BackgroundTrigger) => string> = {
  ci_failure: (trigger) => (trigger.branch ? `CI failed on ${trigger.branch}` : "CI failed"),
  issue_label: () => "An issue got a label",
  schedule: () => "On a schedule",
  dependency_update: () => "A dependency update opened",
  watch: () => "Watch",
};

/** One row of Recent runs: only what the route reports. */
export function runRow(run: WireRun, crew: WireCrew, now: Date): BackgroundRun {
  const ended = run.ended_at ?? run.updated_at;
  // The design's three results: "Finished 15:49 · 12 turns, 9 min", "Running · 6 min" and "40 of 40 turns · 31 of 60 min".
  const result = run.state === "running" ? `Running · ${run.minutes_spent} min`
    : run.state === "stopped" ? `${run.turns_spent} of ${run.budget.turns} turns · ${run.minutes_spent} of ${run.budget.minutes} min`
    : `Finished ${whenShort(ended, now)} · ${spent(run)}`;
  return {
    id: run.id,
    name: crew.name,
    template: TEMPLATE_OF[crew.template],
    startedBy: `${STARTED_BY[crew.trigger.kind](crew.trigger)} · ${whenShort(run.started_at, now)}`,
    result,
    state: run.state === "running" ? "Running" : run.state === "stopped" ? "Stopped" : "Needs review",
    reason: run.state === "stopped" ? reasonOf(run) : null,
    mode: null,
  };
}

export function crewRow(crew: WireCrew, newest: WireRun | undefined): BackgroundCrew {
  return {
    id: crew.id,
    name: crew.name,
    template: TEMPLATE_OF[crew.template],
    startsWhen: startsWhen(crew),
    lastRun: lastRunLine(newest),
    enabled: crew.enabled,
    status: crewStatus(crew, newest),
    mode: null,
  };
}

/** What one read of the Background page returns: the crews, and each crew's newest runs. */
export interface BackgroundRead {
  crews: WireCrew[];
  /** How many crews the route holds; more than `crews` when the read stopped at its page limit. */
  total: number;
  runs: Record<string, WireRun[]>;
}

/** The runs the page lists. */
export const RECENT_RUNS = 10;

export function backgroundView(read: BackgroundRead, now: Date): BackgroundView {
  const byId = new Map(read.crews.map((crew) => [crew.id, crew]));
  const configured: Partial<Record<TriggerId, string>> = {};
  for (const crew of read.crews) {
    const line = configuredAs(crew.trigger);
    const id = TRIGGER_OF[crew.trigger.kind];
    if (line && !configured[id]?.split(", ").includes(line)) configured[id] = configured[id] ? `${configured[id]}, ${line}` : line;
  }
  const runs = read.crews
    .flatMap((crew) => (read.runs[crew.id] ?? []).map((run) => ({ run, crew })))
    .sort((a, b) => Date.parse(b.run.started_at) - Date.parse(a.run.started_at))
    .slice(0, RECENT_RUNS)
    .map(({ run, crew }) => runRow(run, byId.get(crew.id) ?? crew, now));
  return { crews: read.crews.map((crew) => crewRow(crew, read.runs[crew.id]?.[0])), total: read.total, runs, configured };
}
