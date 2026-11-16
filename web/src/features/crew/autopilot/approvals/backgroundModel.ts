// What the Crew API says about background crews, as the words and rows the Background screens draw (Crew designs: background-list,
// background-runs, background-triggers). Pure: the same reads give the same rows. The wire names and the design's template and trigger
// ids differ, so the two maps below are the only place that knows both.
import type { BackgroundCrew as WireCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun as WireRun } from "../../../../generated/BackgroundRun";
import type { BackgroundTemplate } from "../../../../generated/BackgroundTemplate";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import type { BackgroundTriggerKind } from "../../../../generated/BackgroundTriggerKind";
import type { BackgroundBudget } from "../../../../generated/BackgroundBudget";
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
  /** The crew's own Autopilot setting. The route does not report one, so the column is drawn only when every crew has it. */
  mode: Mode | null;
}
export interface BackgroundRun {
  id: string;
  name: string;
  template: TemplateId;
  startedBy: string;
  result: string;
  /** Null when the route reports no state the design draws for the run (it landed in Review). */
  state: "Merged" | "Running" | "Stopped" | "Failed" | null;
  mode: Mode | null;
}
export interface BackgroundView {
  crews: BackgroundCrew[];
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
};
export const WIRE_TRIGGER: Record<TriggerId, BackgroundTriggerKind> = {
  ci_fails: "ci_failure",
  issue_label: "issue_label",
  schedule: "schedule",
  dependency_update: "dependency_update",
};

/**
 * The budget a new crew starts with, in the route's own units (agent turns and minutes). The designs draw tokens per engine, which
 * this route does not take: an open question for the designer, so this stays one constant until it is answered.
 */
export const DEFAULT_BUDGET: BackgroundBudget = { turns: 40, minutes: 30 };
export const budgetLine = (budget: BackgroundBudget) => `${budget.turns} turns · ${budget.minutes} minutes`;

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

/** "When CI fails on main": the trigger of a crew, in words. A security advisory crew reads as the advisory, as the design draws it. */
export function startsWhen(crew: Pick<WireCrew, "template" | "trigger">): string {
  const { trigger } = crew;
  switch (trigger.kind) {
    case "ci_failure":
      return trigger.branch ? `When CI fails on ${trigger.branch}` : "When CI fails";
    case "issue_label":
      return `When an issue gets the label ${trigger.label ?? ""}`.trim();
    case "schedule":
      return trigger.every_hours === 1 ? "Every hour" : `Every ${trigger.every_hours ?? "?"} hours`;
    case "dependency_update":
      return crew.template === "security_advisory" ? "When a security advisory is published" : "When a dependency update opens";
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
      return trigger.ecosystem || null;
  }
}

const STOP_REASON: Record<NonNullable<WireRun["stop_reason"]>, string> = {
  budget: "budget",
  repeated_failure: "repeated failure",
  engine_error: "engine error",
  disabled: "turned off",
};

/** How a run ended, as the Last run and Result cells say it. */
function outcome(run: WireRun): string {
  if (run.state === "running") return `Running · ${run.minutes_spent} min`;
  if (run.state === "stopped") return `Stopped · ${run.stop_reason ? STOP_REASON[run.stop_reason] : "stopped"}`;
  return "Landed in Review";
}

/** "Sat 7 Nov, 15:11 · landed in Review": the newest run of a crew, or "Never run". */
export function lastRunLine(run: WireRun | undefined): string | null {
  if (!run) return null;
  const how = outcome(run);
  return `${whenLong(run.started_at)} · ${how.charAt(0).toLowerCase()}${how.slice(1)}`;
}

const STARTED_BY: Record<BackgroundTriggerKind, (trigger: BackgroundTrigger) => string> = {
  ci_failure: (trigger) => (trigger.branch ? `CI failed on ${trigger.branch}` : "CI failed"),
  issue_label: () => "An issue got a label",
  schedule: () => "On a schedule",
  dependency_update: () => "A dependency update opened",
};

/** One row of Recent runs. The run's state is a Stopped or Running chip; a run that landed in Review has no state the route reports. */
export function runRow(run: WireRun, crew: WireCrew, now: Date): BackgroundRun {
  const ended = run.ended_at ?? run.updated_at;
  const result = run.state === "needs_review" ? `Landed in Review · ${whenShort(ended, now)}` : outcome(run);
  return {
    id: run.id,
    name: crew.name,
    template: TEMPLATE_OF[crew.template],
    startedBy: `${STARTED_BY[crew.trigger.kind](crew.trigger)} · ${whenShort(run.started_at, now)}`,
    result,
    state: run.state === "running" ? "Running" : run.state === "stopped" ? "Stopped" : null,
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
    mode: null,
  };
}

/** What one read of the Background page returns: the crews, and each crew's newest runs. */
export interface BackgroundRead {
  crews: WireCrew[];
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
  return { crews: read.crews.map((crew) => crewRow(crew, read.runs[crew.id]?.[0])), runs, configured };
}
