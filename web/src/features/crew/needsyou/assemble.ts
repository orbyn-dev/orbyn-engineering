// Assembles the Needs you inbox from the routes that exist: held commands, stops, the after-merge Review queue and the
// background runs waiting for review. Pure: the same reads give the same rows, in the same order.
import type { AutopilotAreaRow } from "../../../generated/AutopilotAreaRow";
import type { AutopilotLogRow } from "../../../generated/AutopilotLogRow";
import type { AutopilotSecondEngineNotice } from "../../../generated/AutopilotSecondEngineNotice";
import type { AutopilotReviewItem } from "../../../generated/AutopilotReviewItem";
import type { BackgroundCrew } from "../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../generated/BackgroundRun";
import type { FlowCard } from "../../../generated/FlowCard";
import { modeName, plural } from "../autopilot/approvals/areas";
import { stopOf } from "../autopilot/approvals/ledger";
import type { ApprovalRow, Risk } from "../autopilot/approvals/model";
import type { WatchNeedsItem } from "../autopilot/approvals/watchTypes";
import { whenLong } from "../autopilot/approvals/when";
import { diskLowOf, diskSentence, NO_WORKER_RAN } from "../tool";
import { KIND_FILTER, NEEDS_KINDS, type NeedsItem, type NeedsKind, type NeedsView } from "./model";

const time = (text: string | null | undefined) => {
  const at = text ? Date.parse(text) : Number.NaN;
  return Number.isNaN(at) ? null : at;
};

/** Existing approval-one-vendor frame, pinned to design Git a582a9ba4fb9.
 * Ordinary notices have Sign in and Dismiss; guarded-command authority is separate. */
export function secondEngineItems(notices: AutopilotSecondEngineNotice[]): NeedsItem[] {
  return notices.map((notice) => ({
    id: `second-engine:${notice.id}`, kind: "denied", title: `Sign in to a second engine to allow: ${notice.target}`,
    risk: null, undo: null, from: `Task ${notice.task_id}`, since: time(notice.at),
    cost: "The worker is told to take another approach, and the rest of the crew carries on.",
    actions: [{ key: "sign-in", label: "Sign in to a second engine", primary: true }, { key: "dismiss", label: "Dismiss" }],
    ref: String(notice.id),
  }));
}

/** A held command that still waits for the person. A decided, expired or dismissed one is not an item. */
export function heldItems(rows: ApprovalRow[], now: number): NeedsItem[] {
  return rows.flatMap((row) => {
    const held = row.held;
    if (!held || held.state !== "held") return [];
    return [
      {
        id: `held:${held.id}`,
        kind: "approval" as const,
        title: row.title,
        risk: null,
        undo: null,
        from: `Task ${row.taskId ?? ""}`.trim(),
        cost: null,
        since: row.at ? (time(row.at) ?? now) : null,
        actions: [
          { key: "approve" as const, label: "Approve", primary: true },
          { key: "deny" as const, label: "Deny" },
        ],
        ref: held.id,
      },
    ];
  });
}

/** What the card says when the service gave no reason. */
export const NEVER_RAN = "The worker never ran.";

/**
 * A could-not-start card with the refusal's structured fields, when the service sends them: the
 * `CrewError`'s code and the two numbers (never a path). A card without them is an older answer and
 * keeps the reason it carries.
 */
export type CouldNotStartCard = FlowCard & {
  code?: string | null;
  free_bytes?: number | null;
  floor_gb?: number | null;
};

/**
 * Tasks refused at their start, from the Needs you column of each crew's board. Not an approval: nothing is asked of the person but
 * to retry the task on another engine or dismiss it, so the item has no Approve or Deny. The reason is the service's own redacted
 * text, drawn as text. A `disk_low` refusal is read from its code and numbers, never from that sentence: it writes the disk
 * sentence, says what did not happen, and offers Dismiss only (another engine writes to the same disk).
 */
export function couldNotStartItems(cards: CouldNotStartCard[]): NeedsItem[] {
  return cards
    .filter((card) => card.could_not_start)
    .map((card) => {
      const lowDisk = diskLowOf(card);
      return {
        id: `task:${card.id}`,
        kind: "could_not_start" as const,
        // The task's name: the narrow card's title (its kind label says "Could not start"). The wide row keeps the drawn frame's "Could not start" in bold (NeedsRow).
        title: card.name?.trim() || card.id,
        risk: null,
        undo: null,
        from: `Task ${card.name?.trim() || card.id}${card.engine ? ` · ${card.engine}` : ""}`,
        cost: lowDisk ? diskSentence(lowDisk.free_bytes, lowDisk.floor_gb) : (card.reason?.trim() || NEVER_RAN),
        hint: lowDisk ? NO_WORKER_RAN : undefined,
        since: time(card.updated_at),
        engine: card.engine ?? undefined,
        actions: lowDisk
          ? [{ key: "dismiss" as const, label: "Dismiss" }]
          : [{ key: "retry" as const, label: "Retry on another engine", primary: true }, { key: "dismiss" as const, label: "Dismiss" }],
        ref: card.id,
      };
    });
}

/** Dismissing a stop is local to this device: nothing is sent to the service. */
export const STOP_HINT = "Dismiss hides this stop on this device.";

/** Tasks Crew stopped on a trip wire, from `GET /api/crew/stops`. The service lists the latest wires; none says it was answered. */
export function stopItems(recent: AutopilotLogRow[], limits: { inARow: number; inARun: number }): NeedsItem[] {
  return recent
    .filter((row) => row.kind === "tripwire")
    .map((row) => {
      const stop = stopOf(row, limits);
      return {
        // Ledger id, task and time together: an id reused by another service or a reset database is another stop.
        id: `stop:${row.id}:${row.task_id}:${row.at}`,
        kind: "stopped" as const,
        title: stop.reason === "injected_instructions" ? "Stopped · possible injected instructions" : "Stopped · too many denials",
        risk: null,
        undo: null,
        from: `Task ${row.task_id}`,
        // The reason in words, as Crew Designs v37 words it.
        cost:
          stop.reason === "injected_instructions"
            ? "A page the worker read contained text that looks like instructions. The task stopped before it ran anything the text asked for."
            : `${stop.denialCount} denials ${stop.inARun ? "in one run" : "in a row"} stopped the task. Nothing else is blocked; the other tasks keep going.`,
        since: time(row.at),
        hint: STOP_HINT,
        actions: [{ key: "open" as const, label: "Open", primary: true }, { key: "dismiss" as const, label: "Dismiss" }],
        ref: row.task_id,
      };
    });
}

/** Only a break the crew was not certain enough to revert needs the person here. Other merges wait in the Review tab, not in Needs you. */
export function mainItems(queue: AutopilotReviewItem[]): NeedsItem[] {
  return queue
    .filter((item) => item.needs_person != null && item.needs_person !== "")
    .map((item) => ({
      id: `main:${item.task_id}`,
      kind: "main" as const,
      title: "Main broke and the cause isn’t certain",
      risk: item.risk as Risk,
      undo: null,
      from: `Watch on main · ${item.merge_commit.slice(0, 7)}`,
      cost: "Nothing has been reverted. Main stays as it is until you choose.",
      overdue: item.overdue,
      since: item.merged_unix ? item.merged_unix * 1000 : null,
      actions: [{ key: "open" as const, label: "Open", primary: true }],
      ref: item.task_id,
    }));
}

/**
 * A daily-cap stop, told apart from the lifetime budget stop by the core's own sentence: only the daily stops (the cap, or a daily
 * budget that could not be read) say "daily budget". The read has no flag for it, so the sentence is the evidence.
 */
const DAILY_STOP = /daily budget/i;
/** The reset time the core's sentence ends with ("The budget resets at 2026-11-20T00:00:00+11:00."), as read; null when the sentence has none. */
const RESET_AT = /resets at (\S+?)\.?$/;

/** Background runs that wait for the person's review, with the crew each belongs to. */
export function backgroundItems(runs: { crew: BackgroundCrew; runs: BackgroundRun[] }[]): NeedsItem[] {
  return runs.flatMap(({ crew, runs: list }) =>
    list
      .filter((run) => run.review_waiting)
      .map((run) => {
        const reason = run.review_reason?.trim() ?? "";
        const daily = run.stop_reason === "budget" && DAILY_STOP.test(reason);
        const reset = daily ? RESET_AT.exec(reason)?.[1] : undefined;
        return {
          id: `background:${run.id}`,
          kind: "background" as const,
          // A run that spent its budget is paused; any other run waiting on the person needs approval.
          title: run.stop_reason === "budget" ? "Paused: budget reached" : "Needs approval",
          risk: null,
          undo: null,
          from: `Background crew · ${crew.name}`,
          // A daily stop shows the core's sentence as read; the lifetime budget stop keeps the line for a budget that cannot be raised.
          cost: daily
            ? reason
            : run.stop_reason === "budget"
              ? "The run stopped at its budget. The budget cannot be changed; turn the crew off, or create a new crew with a bigger budget."
              : (reason || "A command is held for your approval before the run can go on."),
          // The next step of a daily stop is to wait; the time is the one the sentence carries, never one made up here.
          hint: daily
            ? reset
              ? `This run stays stopped. New runs can start after the daily reset: ${reset}. The budget is fixed at creation.`
              : "No reset time: the daily budget could not be read or updated, so this is not a wait for the reset. Open the run to review it."
            : undefined,
          since: time(run.ended_at ?? run.updated_at),
          actions: [{ key: "open" as const, label: "Open", primary: true }],
          ref: crew.id,
        };
      }),
  );
}

/** The streak a could-not-run reason opens with ("3 checks in a row could not run: timed out."), as read; null when it has none. */
const STREAK = /^(\d+) checks? in a row/;

/**
 * The two watch rows of the Needs you read (KC6, watch-needsyou): a watch whose command turned failing and asked for the
 * person, and one whose last three checks in a row could not run. Both settle themselves on a check that runs again, so
 * neither is dismissible; each action opens the watch itself. The core's one redacted line is the reason, and the design's
 * sentence says how it settles. A watch whose crew is not in the list keeps its id's row without a name it cannot learn.
 */
export function watchItems(read: { items: WatchNeedsItem[]; names: Record<string, string> }): NeedsItem[] {
  return read.items.map((item) => {
    const name = read.names[item.crew_id ?? item.id] ?? item.id;
    const reason = item.reason?.trim() || (item.kind === "watch_failing" ? "The watch’s command is failing." : "Checks in a row could not run.");
    const streak = item.kind === "watch_could_not_run" ? STREAK.exec(reason)?.[1] : undefined;
    const failing = item.kind === "watch_failing";
    return {
      id: `watch:${item.id}`,
      kind: "background" as const,
      title: failing ? `Watch failing: ${name}` : `Watch could not run: ${name}`,
      risk: null,
      undo: null,
      // `since` is the latest check (the service's row time), not the streak's first error: only a "one row since" claim is true.
      from: failing
        ? `Watch · ${name} · turned failing ${whenLong(item.since)}`
        : `Watch · ${name} · ${streak ? `${streak} checks in a row · one row since` : "one row since"} ${whenLong(item.since)}`,
      cost: failing
        ? `${reason} This is raised once for this change and is meant not to repeat while the check keeps failing. It resolves itself when a check passes.`
        : `${reason} One row for the whole stretch: it is updated at each later check, not repeated, and it resolves itself when a check passes.`,
      since: time(item.since),
      actions: [{ key: "open" as const, label: "Open", primary: true }],
      ref: item.crew_id ?? item.id,
    };
  });
}

const RISK_RANK: Record<Risk, number> = { high: 0, medium: 1, low: 2 };

/** The kinds that ask nothing of the person (needsyou-demoted: a demotion blocks nothing), ranked below every waiting item. */
const BLOCKS_NOTHING: ReadonlySet<NeedsKind> = new Set(["autonomy"]);

/**
 * An area the circuit breaker demoted, as the Needs you inbox's one line (Crew Designs: needsyou-demoted). The route's record is the
 * trip's own numbers, so the line names them. Only a demotion under a Full ceiling is a row: a recovering or criteria-met area is not a
 * new tell (the areas page keeps showing it, and its served record is the recovery, not the trip), a ceiling below Full means the
 * demotion changed nothing, and the inbox never re-tells an area that follows its ceiling. Only a demotion to On is a row, since the
 * line says a High risk diff waits while Low and Medium still merge; an area the breaker took to Off says something else.
 */
export function autonomyItems(rows: AutopilotAreaRow[], project: string): NeedsItem[] {
  return rows
    .filter((row) => row.state === "demoted" && row.ceiling === "full" && row.now === "on")
    .map((row) => ({
      // The repository and the trip time are part of the id, like a stop's: a new trip after a dismissal is a new row, and another
      // repository's area of the same name is another row.
      id: `autonomy:${project}:${row.area_key}:${row.since ?? ""}`,
      kind: "autonomy" as const,
      title: `${row.area} moved from ${modeName(row.ceiling)} to ${modeName(row.now)}: ${plural(row.record.reverts, "revert")} in ${row.record.window_days} days. Review or override.`,
      risk: null,
      undo: null,
      from: `Autopilot by area · circuit breaker · ${plural(row.record.merges, "merge")} in the window`,
      cost: `Nothing is blocked. A High risk diff in ${row.area} now waits for you at the merge gate; Low and Medium still merge by themselves. Dismiss is remembered on this browser.`,
      since: row.since ? row.since * 1000 : null,
      actions: [
        { key: "review" as const, label: "Review", primary: true },
        { key: "override" as const, label: "Override…" },
        { key: "dismiss" as const, label: "Dismiss" },
      ],
      ref: row.area_key,
    }));
}

/**
 * The inbox's order: risk first, for the items whose source states one; then, within one risk tier, the kinds that block
 * nothing (a demotion) below the kinds that wait on the person (needsyou-demoted); then the oldest first (docs/crew.md), an
 * unknown time last, then the id so a tie never moves. Reversibility and the cost of waiting are ranked by no source today.
 */
export function rank(items: NeedsItem[]): NeedsItem[] {
  return [...items].sort((a, b) => {
    const risk = (a.risk ? RISK_RANK[a.risk] : 3) - (b.risk ? RISK_RANK[b.risk] : 3);
    if (risk) return risk;
    const waiting = Number(BLOCKS_NOTHING.has(a.kind)) - Number(BLOCKS_NOTHING.has(b.kind));
    if (waiting) return waiting;
    if (a.since !== b.since) return (a.since ?? Number.POSITIVE_INFINITY) - (b.since ?? Number.POSITIVE_INFINITY);
    return a.id.localeCompare(b.id);
  });
}

/** The filter chips: one for each kind that has an item, in the designs' order. */
export function viewOf(items: NeedsItem[]): NeedsView {
  const filters = NEEDS_KINDS.flatMap((kind) => {
    const count = items.filter((item) => item.kind === kind).length;
    return count ? [{ kind, label: KIND_FILTER[kind], count }] : [];
  });
  return { items, filters };
}

export const only = (items: NeedsItem[], kind: NeedsKind | "all") => (kind === "all" ? items : items.filter((item) => item.kind === kind));
