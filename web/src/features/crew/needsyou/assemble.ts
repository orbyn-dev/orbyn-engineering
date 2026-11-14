// Assembles the Needs you inbox from the routes that exist: held commands, stops, the after-merge Review queue and the
// background runs waiting for review. Pure: the same reads give the same rows, in the same order.
import type { AutopilotLogRow } from "../../../generated/AutopilotLogRow";
import type { AutopilotReviewItem } from "../../../generated/AutopilotReviewItem";
import type { BackgroundCrew } from "../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../generated/BackgroundRun";
import { stopOf } from "../autopilot/approvals/ledger";
import type { ApprovalRow, Risk } from "../autopilot/approvals/model";
import { KIND_FILTER, NEEDS_KINDS, type NeedsItem, type NeedsKind, type NeedsView } from "./model";

const time = (text: string | null | undefined) => {
  const at = text ? Date.parse(text) : Number.NaN;
  return Number.isNaN(at) ? null : at;
};

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

/** Tasks Crew stopped on a trip wire, from `GET /api/crew/stops`. The service lists the latest wires; none says it was answered. */
export function stopItems(recent: AutopilotLogRow[], limits: { inARow: number; inARun: number }): NeedsItem[] {
  return recent
    .filter((row) => row.kind === "tripwire")
    .map((row) => {
      const stop = stopOf(row, limits);
      return {
        id: `stop:${row.id}`,
        kind: "stopped" as const,
        title: stop.reason === "injected_instructions" ? "Stopped · possible injected instructions" : "Stopped · too many denials",
        risk: null,
        undo: null,
        from: `Task ${row.task_id}`,
        // The reason in words, from the Approvals tab's own sentences.
        cost:
          stop.reason === "injected_instructions"
            ? "It read text that looks like instructions to the agent, so Crew stopped it. Nothing was lost; its worktree is intact."
            : `It was denied ${stop.denialCount} times ${stop.inARun ? "in one run" : "in a row"}, so Crew stopped it. Nothing was lost; its worktree is intact.`,
        since: time(row.at),
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

/** Background runs that wait for the person's review, with the crew each belongs to. */
export function backgroundItems(runs: { crew: BackgroundCrew; runs: BackgroundRun[] }[]): NeedsItem[] {
  return runs.flatMap(({ crew, runs: list }) =>
    list
      .filter((run) => run.review_waiting)
      .map((run) => ({
        id: `background:${run.id}`,
        kind: "review" as const,
        // A run that spent its budget is paused; any other run waiting on the person says what it waits for, or that it needs approval.
        title: run.stop_reason === "budget" ? "Paused: budget reached" : run.review_reason?.trim() ? `Review: ${run.review_reason.trim()}` : "Needs approval",
        risk: null,
        undo: null,
        from: `Background · ${crew.name}`,
        cost: run.stop_reason === "budget" ? (run.review_reason?.trim() ?? null) : null,
        since: time(run.ended_at ?? run.updated_at),
        actions: [{ key: "open" as const, label: "Open", primary: true }],
        ref: crew.id,
      })),
  );
}

const RISK_RANK: Record<Risk, number> = { high: 0, medium: 1, low: 2 };

/**
 * The inbox's order: risk first, for the items whose source states one, then the items without a stated risk; then the oldest
 * first (docs/crew.md), an unknown time last, then the id so a tie never moves. Reversibility and the cost of waiting are
 * ranked by no source today.
 */
export function rank(items: NeedsItem[]): NeedsItem[] {
  return [...items].sort((a, b) => {
    const risk = (a.risk ? RISK_RANK[a.risk] : 3) - (b.risk ? RISK_RANK[b.risk] : 3);
    if (risk) return risk;
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
