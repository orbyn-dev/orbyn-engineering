// What a watch's reads mean, as the words and rows the Background screens draw. Crew designs: watch-list, watch-passing,
// watch-failing, watch-failing-template, watch-still-failing, watch-recovered, watch-paused, watch-command-error and
// watch-history. Pure: the same reads give the same words. The contract is the open service pull request (KC6), read
// through ./watchTypes; nothing here invents a field it does not carry.
import type { BackgroundCrew as WireCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import { templateNameOf } from "./copy";
import type { ChipTone } from "./parts";
import type { BackgroundWatchChange, BackgroundWatchStatus } from "./watchTypes";
import { clockOf, dayOfIso, plusHours, sameMoment, whenLong, whenMoment, whenShort } from "./when";

/** The crew fields a watch draws: its name, its template, its trigger (the command and the schedule) and the switch. */
export type WatchCrew = Pick<WireCrew, "name" | "template" | "trigger" | "enabled">;

export interface WatchChip {
  text: "Not checked yet" | "Paused" | "Passing" | "Failing" | "Could not run";
  tone: ChipTone;
}

export interface WatchListRow {
  chip: WatchChip;
  /** The list's "Last run or check" cell. */
  lastLine: string;
  /** The line under the name: "Watch · no model · npm test web/nav/redirect". */
  subLine: string;
  /** The list's "Starts when" cell: "Every 1 hour · raises Needs you". */
  startsWhen: string;
}

export interface WatchFact {
  label: string;
  text: string;
}

export interface WatchBanner {
  title: string;
  text: string;
  /** The one action the banner offers, or null. */
  action: "needs_you" | "run" | null;
}

export interface WatchTail {
  title: string;
  body: string;
}

export interface WatchChangeRow {
  id: number;
  when: string;
  change: string;
  exit: string;
  did: string;
}

export interface WatchChangeDetail {
  title: string;
  body: string;
}

export interface WatchChangesView {
  rows: WatchChangeRow[];
  note: string | null;
  /** The newest change that kept a tail, as the history frame draws it under the list. */
  detail: WatchChangeDetail | null;
}

export interface WatchView {
  chip: WatchChip;
  banner: WatchBanner;
  facts: WatchFact[];
  tail: WatchTail | null;
  changes: WatchChangesView;
}

/** "Every 1 hour" / "Every 6 hours": the watch schedule in the design's words (watch-setup and watch-list). */
export const everyHours = (hours: number | null | undefined): string => `Every ${hours ?? 1} ${hours === 1 ? "hour" : "hours"}`;

/** The action a failing check takes: "Raise Needs you" or "Start the crew template". */
export const watchAction = (trigger: BackgroundTrigger): "needs_you" | "template" =>
  trigger.kind === "watch" && trigger.on_failing === "template" ? "template" : "needs_you";

const commandOf = (trigger: BackgroundTrigger): string => (trigger.kind === "watch" ? (trigger.command ?? "") : "");

/** The list's Starts when cell: "Every 24 hours · raises Needs you", "Every 1 hour · starts Fix failing CI". */
export function watchStartsWhen(crew: WatchCrew): string {
  const action = watchAction(crew.trigger) === "template" ? `starts ${templateNameOf(crew.template)}` : "raises Needs you";
  return `${everyHours(crew.trigger.every_hours)} · ${action}`;
}

/** The line under a watch's name in the list. */
export const watchSubLine = (crew: WatchCrew): string => ["Watch · no model", commandOf(crew.trigger)].filter(Boolean).join(" · ");

/** The chip: Not checked yet, Paused, Passing, Failing or Could not run, as the designs draw them. */
export function watchChip(crew: WatchCrew, status: BackgroundWatchStatus | null): WatchChip {
  if (!status || !status.baseline_done) return { text: "Not checked yet", tone: "dim" };
  if (!crew.enabled) return { text: "Paused", tone: "dim" };
  switch (status.state) {
    case "passing":
      return { text: "Passing", tone: "ok" };
    case "failing":
      return { text: "Failing", tone: "no" };
    case "could_not_run":
      return { text: "Could not run", tone: "am" };
    default:
      return { text: "Not checked yet", tone: "dim" };
  }
}

/** The newest change of one kind, or undefined. */
const changeOf = (changes: BackgroundWatchChange[], kind: BackgroundWatchChange["kind"]) => changes.find((one) => one.kind === kind);

/** "Sat 7 Nov at 14:02": the design's own punctuation for a moment named inside a sentence. */
const whenAt = (iso: string): string => `${dayOfIso(iso)} at ${clockOf(iso)}`;

/** The list's "Last run or check": the state and what the last check found, in the design's words. */
export function watchLastLine(crew: WatchCrew, status: BackgroundWatchStatus | null, changes: BackgroundWatchChange[], now: Date): string {
  if (!status || !status.baseline_done || !status.last_checked_at) return "Not checked yet";
  const at = status.last_checked_at;
  if (!crew.enabled) return `Last checked ${whenLong(at)}`;
  if (status.state === "could_not_run") return `Checked ${whenShort(at, now)} · could not run: ${status.reason ?? "it could not run"}`;
  if (status.state === "failing") {
    const raised = changeOf(changes, "failing");
    const since = raised && !sameMoment(raised.at, at) ? ` · failing since ${whenShort(raised.at, now)}` : "";
    return `Checked ${whenShort(at, now)} · exit ${status.last_exit_code ?? "unknown"}${since}`;
  }
  return `Checked ${whenShort(at, now)} · next ${whenShort(plusHours(at, crew.trigger.every_hours ?? 1), now)}`;
}

/** One list row of a watch. */
export function watchRow(crew: WatchCrew, status: BackgroundWatchStatus | null, changes: BackgroundWatchChange[], now: Date): WatchListRow {
  return { chip: watchChip(crew, status), lastLine: watchLastLine(crew, status, changes, now), subLine: watchSubLine(crew), startsWhen: watchStartsWhen(crew) };
}

const stateAtBaseline = (change: BackgroundWatchChange): string => (change.exit_code === 0 ? "Passing" : "Failing");

const watchChangeLabel = (change: BackgroundWatchChange): string =>
  change.kind === "baseline" ? `Baseline: ${stateAtBaseline(change)}` : change.kind === "failing" ? "Passing → Failing" : change.kind === "passing" ? "Passing again" : "Turned off by you";

/** One row of the change table, in the design's words: When, Change, Exit, What it did. */
export function watchChangeRow(change: BackgroundWatchChange, changes: BackgroundWatchChange[], crew: WatchCrew, now: Date): WatchChangeRow {
  const where = changes.findIndex((one) => one.id === change.id);
  const raised = where < 0 ? undefined : changes.slice(where + 1).find((one) => one.kind === "failing");
  const did =
    change.action === "needs_you"
      ? "Raised in Needs you."
      : change.action === "started_template"
        ? `Started the ${templateNameOf(crew.template)} template.`
        : change.action === "resolved"
          ? raised
            ? `Resolved the Needs you row from ${whenMoment(raised.at, now)}.`
            : "Resolved the Needs you row."
          : change.kind === "baseline"
            ? "Recorded. A first check never raises."
            : change.kind === "turned_off"
              ? "No more checks."
              : "Recorded.";
  return { id: change.id, when: whenLong(change.at), change: watchChangeLabel(change), exit: change.exit_code == null ? "—" : String(change.exit_code), did };
}

const tailTitle = (change: BackgroundWatchChange): string =>
  `Change detail · ${whenLong(change.at)} · ${watchChangeLabel(change)} · exit ${change.exit_code ?? "unknown"} · the last 20 lines, 2 KiB at most, secrets hidden`;

/** The Changes section: its rows, its foot line and the newest kept tail under the table (watch-history). */
export function watchChangesView(changes: BackgroundWatchChange[], crew: WatchCrew, now: Date, status: BackgroundWatchStatus | null = null, withDetail = true): WatchChangesView {
  const rows = changes.map((one) => watchChangeRow(one, changes, crew, now));
  // The change-detail box is its own frame (watch-history); a watch's page draws the latest tail once, above the table.
  const kept = withDetail ? changes.find((one) => one.tail != null && one.tail !== "") : undefined;
  const note =
    status?.state === "could_not_run"
      ? "Could not run is not a change of result and has no row here."
      : status?.state === "failing" && changes[0]?.kind === "failing" && status.last_checked_at && !sameMoment(changes[0].at, status.last_checked_at)
        ? "Checks that found the same result as the one before are not listed."
        : null;
  return { rows, note, detail: kept ? { title: tailTitle(kept), body: kept.tail! } : null };
}

/** The banner at the top of a watch's page: the state, what happened, and the one action it offers. */
export function watchBanner(crew: WatchCrew, status: BackgroundWatchStatus, changes: BackgroundWatchChange[], now: Date): WatchBanner {
  const at = status.last_checked_at;
  const newest = changes[0];
  const raised = changeOf(changes, "failing");
  const previousFailing = (() => {
    if (!newest) return undefined;
    const where = changes.findIndex((one) => one.id === newest.id);
    return where < 0 ? undefined : changes.slice(where + 1).find((one) => one.kind === "failing");
  })();
  const turned = sameMoment(newest?.at, at);
  const clock = at ? clockOf(at) : "";
  if (!crew.enabled) {
    // The last result is drawn in the facts box, never inferred here: a paused watch may have failed only at its baseline, and that raised nothing.
    return {
      title: "Paused: it is off",
      text: "No check runs while it is off. When you turn it on, the next check is compared with that last result. A change during the pause is acted on once, on the first check after you turn it on.",
      action: null,
    };
  }
  if (!status.baseline_done) {
    return {
      title: "Not checked yet",
      text: "The first check after you turn it on is a baseline: it is recorded and never raises, even if it fails.",
      action: null,
    };
  }
  if (status.state === "could_not_run") {
    const streak = status.error_streak;
    return {
      title: `Could not run: ${status.reason ?? "it could not run"}`,
      text: `The last check could not run: ${status.reason ?? "it could not run"}. This is not a failing result. ${
        streak >= 3
          ? "Three checks in a row could not run, so one row is in Needs you; later errors update that row and do not add another."
          : `${streak} ${streak === 1 ? "check" : "checks"} in a row could not run; after three in a row, one row is raised in Needs you.`
      }`,
      action: null,
    };
  }
  if (status.state === "failing") {
    const exit = status.last_exit_code ?? "unknown";
    // A check that fails first is the baseline: it sets Failing and raises nothing (watch-failing draws the raised change only).
    if (!raised) {
      return {
        title: "Failing: baseline",
        text: `The first check, on ${at ? whenAt(at) : "its first run"}, exited ${exit}. A first failing check never raises: the watch reads Failing, and no Needs you row opens until a check fails after a pass.`,
        action: null,
      };
    }
    if (turned && newest?.kind === "failing") {
      return newest.action === "started_template"
        ? {
            title: `Failing: started ${templateNameOf(crew.template)}`,
            text: `It turned failing on ${whenAt(newest.at)}: the check exited ${exit}. That change started one run of the ${templateNameOf(crew.template)} template at ${clockOf(newest.at)}.`,
            action: "run",
          }
        : {
            title: "Failing: raised in Needs you",
            text: `It turned failing on ${whenAt(newest.at)}: the check exited ${exit}. That change was raised once, in Needs you, at ${clockOf(newest.at)}. The row resolves itself when a check passes.`,
            action: "needs_you",
          };
    }
    return {
      title: "Still failing: already raised",
      text: `The latest check, at ${clock}, exited ${exit}. It raised once, at ${clockOf(raised.at)}, and is meant not to raise again until a check passes and a later one fails.`,
      action: null,
    };
  }
  if (turned && newest?.kind === "passing" && newest.action === "resolved" && previousFailing) {
    return {
      title: "Passing again",
      text: `The check at ${clock} exited 0. It was failing from ${whenMoment(previousFailing.at, now)} to ${clock} and raised once, at ${whenMoment(previousFailing.at, now)}. This pass resolved that Needs you row and wrote the change “Passing again”. If it fails again, that is a new change and raises again.`,
      action: null,
    };
  }
  const baseline = changeOf(changes, "baseline");
  return {
    title: "Passing",
    text: `The last check exited 0. A watch is quiet while the result stays the same: nothing is raised and no run starts.${baseline ? ` Its first check, on ${dayOfIso(baseline.at)}, was a baseline: recorded, never raised.` : ""}`,
    action: null,
  };
}

/** The facts box of a watch's page, in the design's order. */
export function watchFacts(crew: WatchCrew, status: BackgroundWatchStatus, changes: BackgroundWatchChange[], now: Date, repo: string): WatchFact[] {
  const facts: WatchFact[] = [
    { label: "Command", text: commandOf(crew.trigger) },
    { label: "Runs in", text: `${repo} · this device · the project’s checkout root` },
    { label: "Checks", text: `${everyHours(crew.trigger.every_hours)} · no network · 5 minutes at most` },
    { label: "When it turns failing", text: watchAction(crew.trigger) === "template" ? `Start the crew template: ${templateNameOf(crew.template)}` : "Raise Needs you" },
    { label: "Model", text: "None. A check makes no model call." },
  ];
  const at = status.last_checked_at;
  if (!status.baseline_done || !at) {
    facts.push({ label: "Last checked", text: "Not checked yet" });
  } else if (status.state === "could_not_run") {
    facts.push({ label: "Last checked", text: `${whenLong(at)} · could not run: ${status.reason ?? "it could not run"} · no exit code` });
    // The next check is due one schedule step after the last one. The service buckets its ticks on the epoch clock, so this is an
    // approximation of the wall-clock time it will actually fire at, not a schedule the watch keeps any other way.
    facts.push(crew.enabled ? { label: "Next check", text: whenLong(plusHours(at, crew.trigger.every_hours ?? 1)) } : { label: "Next check", text: "None while it is off" });
    facts.push({ label: "Could not run", text: `${status.error_streak} ${status.error_streak === 1 ? "check" : "checks"} in a row` });
    facts.push({ label: "Needs you", text: status.error_streak >= 3 ? "One row, raised after the third check in a row" : "No row yet; three in a row raise one" });
  } else {
    facts.push({ label: "Last checked", text: `${whenLong(at)} · exit ${status.last_exit_code ?? "unknown"}` });
    facts.push(crew.enabled ? { label: "Next check", text: whenLong(plusHours(at, crew.trigger.every_hours ?? 1)) } : { label: "Next check", text: "None while it is off" });
    // A pass right after a failing change is the one that resolved the row (watch-recovered); a later quiet check keeps no line.
    const newest = changes[0];
    const justRecovered = newest && newest.kind === "passing" && newest.action === "resolved" && sameMoment(newest.at, at);
    const previous = justRecovered ? changes.slice(1).find((one) => one.kind === "failing") : undefined;
    if (previous) facts.push({ label: "Needs you", text: `The row from ${whenMoment(previous.at, now)} is resolved` });
  }
  facts.push({ label: "Kept", text: "Exit code, and the last 20 lines of the latest output (2 KiB at most, secrets hidden). Nothing else." });
  return facts;
}

/** The latest output a failing watch kept (watch-failing, watch-still-failing); a passing or could-not-run check keeps none. */
export function watchTail(status: BackgroundWatchStatus): WatchTail | null {
  if (status.state !== "failing" || !status.tail || !status.last_checked_at) return null;
  return {
    title: `Latest output · the check at ${clockOf(status.last_checked_at)} · exit ${status.last_exit_code ?? "unknown"} · the last 20 lines, 2 KiB at most, secrets hidden`,
    body: status.tail,
  };
}

/** Everything a watch's page draws, from its two reads. `repo` is the project's name, as "Runs in" shows it. */
export function watchView(crew: WatchCrew, status: BackgroundWatchStatus, changes: BackgroundWatchChange[], now: Date, repo: string): WatchView {
  return {
    chip: watchChip(crew, status),
    banner: watchBanner(crew, status, changes, now),
    facts: watchFacts(crew, status, changes, now, repo),
    tail: watchTail(status),
    // The page already draws the latest tail, so the change detail stays with the history frame alone.
    changes: watchChangesView(changes, crew, now, status, false),
  };
}
