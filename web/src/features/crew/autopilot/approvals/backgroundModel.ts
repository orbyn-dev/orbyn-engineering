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
import { TEMPLATE_ID_OF } from "./copy";
import type { Mode, TemplateId, TriggerId } from "./model";
import { whenLong, whenShort } from "./when";
import { watchRow, watchStartsWhen, type WatchListRow } from "./watchModel";
import type { BackgroundWatchChange } from "../../../../generated/BackgroundWatchChange";
import type { BackgroundWatchStatus } from "../../../../generated/BackgroundWatchStatus";

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
  /** A watch's row: its state chip and its check line, from the watch reads. A crew that is not a watch, or one whose watch read did not answer, has none. */
  watch?: WatchListRow | null;
}
export interface BackgroundRun {
  id: string;
  name: string;
  template: TemplateId;
  startedBy: string;
  result: string;
  /** The route's three run states. "Merged" and "Failed" are only drawn by the preview views: the route reports neither. */
  state: "Merged" | "Running" | "Stopped" | "Failed" | "Needs review" | "Not started" | null;
  /** Why a stopped run stopped: "Budget", "Repeated failure", "Engine error" or "Turned off"; for a run that never started, "Disk". */
  reason?: string | null;
  /** A muted line under the result: the run never started, so no worker ran. Only a not-started run has one. */
  note?: string | null;
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

export const TEMPLATE_OF: Record<BackgroundTemplate, TemplateId> = TEMPLATE_ID_OF;
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
export const LIMITS = { turns: 1000, minutes: 1440, dailyRuns: 1000, dailyTurns: 10_000, hours: 720, text: 100, name: 80, command: 500 } as const;

/** "When CI fails on main": the trigger of a crew, in words. A watch's own row carries its schedule and action. */
export function startsWhen(crew: Pick<WireCrew, "trigger"> & { template?: WireCrew["template"] }): string {
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
      return crew.template ? watchStartsWhen({ name: "", template: crew.template, trigger, enabled: true }) : everyHoursFallback(trigger.every_hours);
  }
}

/** A watch row read before its template is known: the schedule alone, never an action it cannot name. */
const everyHoursFallback = (hours: number | null | undefined): string => `Every ${hours ?? 1} ${hours === 1 ? "hour" : "hours"}`;

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

/** The reason codes a run that never started can carry, and the one word each draws after "Not started · ". A code not listed is not
 * drawn as a refusal: the run keeps the drawing of the stopped run it is. */
const NOT_STARTED_WORD: Record<string, string> = { disk_low: "Disk" };
/** Crew Designs kc4-low-disk-background-run: the line under the service's sentence. */
const NOT_STARTED_NOTE = "No worker ran. Nothing was removed to make room.";

/** The sentence of a refused start when the service sent none (a proposal without numbers: the service's own sentence carries them). */
const NOT_STARTED_FALLBACK = "Not started: Crew needs more free disk space. Free space and try again.";
/** The word a run that never started carries ("Disk"), or undefined for every other run. */
const notStartedWordOf = (run: WireRun): string | undefined =>
  run.state === "stopped" && !run.stop_reason && run.reason_code ? NOT_STARTED_WORD[run.reason_code] : undefined;

/** "Sat 7 Nov, 15:11 · 12 turns, 9 min": the newest run of a crew, or "Never run". */
export function lastRunLine(run: WireRun | undefined): string | null {
  if (!run) return null;
  // A start the guard refused spent nothing: the cell says so with the chip's own words, not "0 turns, 0 min".
  const refused = notStartedWordOf(run);
  if (refused) return `Not started · ${refused}`;
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
  const notStartedWord = notStartedWordOf(run);
  if (notStartedWord) {
    // The design's row: the service's refusal text word for word, the muted line, and no spend because nothing ran.
    return {
      id: run.id,
      name: crew.name,
      template: TEMPLATE_OF[crew.template],
      startedBy: `${STARTED_BY[crew.trigger.kind](crew.trigger)} · ${whenShort(run.started_at, now)}`,
      result: run.review_reason?.trim() || NOT_STARTED_FALLBACK,
      note: NOT_STARTED_NOTE,
      state: "Not started",
      reason: notStartedWord,
      mode: null,
    };
  }
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

/** A watch's own read, as the list needs it: its last check and the newest change the check line names. */
export interface WatchReadRow {
  status: BackgroundWatchStatus;
  changes: BackgroundWatchChange[];
}

/** One crew's row. A watch draws its state and its check line from its own reads; every other crew draws its newest run. */
export function crewRow(crew: WireCrew, newest: WireRun | undefined, watch: WatchReadRow | undefined, now: Date): BackgroundCrew {
  const watchRowView = crew.trigger.kind === "watch" && watch ? watchRow(crew, watch.status, watch.changes, now) : null;
  return {
    id: crew.id,
    name: crew.name,
    template: TEMPLATE_OF[crew.template],
    startsWhen: watchRowView?.startsWhen ?? startsWhen(crew),
    lastRun: watchRowView?.lastLine ?? lastRunLine(newest),
    enabled: crew.enabled,
    status: watchRowView ? watchRowView.chip.text : crewStatus(crew, newest),
    mode: null,
    watch: watchRowView,
  };
}

/** What one read of the Background page returns: the crews, and each crew's newest runs. */
export interface BackgroundRead {
  crews: WireCrew[];
  /** How many crews the route holds; more than `crews` when the read stopped at its page limit. */
  total: number;
  runs: Record<string, WireRun[]>;
  /** Each watch crew's last check and newest changes. A crew the watch read did not answer for is absent. */
  watches?: Record<string, WatchReadRow>;
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
  return { crews: read.crews.map((crew) => crewRow(crew, read.runs[crew.id]?.[0], read.watches?.[crew.id], now)), total: read.total, runs, configured };
}
