/**
 * The watch wire types, written by hand from the contract in the open service pull request (KC6, slices 2 and 3):
 * `crates/orbyn-contracts/src/background.rs` (`BackgroundWatchState`, `BackgroundWatchStatus`,
 * `BackgroundWatchChange*`) and `crates/orbyn-contracts/src/crew/needs_you.rs` (the two watch kinds of
 * `NeedsYouKind`). There is no generated type for them until that pull request merges. They move to
 * `web/src/generated` then, and this file goes away: until it lands, every shape here is the contract's own.
 */

/** Where a watch's last check stands. The first check is a baseline and raises nothing; after it only a turn between
 *  passing and failing is a change; three checks in a row that could not run raise one Needs you row. */
export type BackgroundWatchState = "not_checked" | "passing" | "failing" | "could_not_run";

/** What one row of a watch's change history records: the first check, a turn to failing, a recovery, or the person
 *  turning the watch off. A different failure while it stays failing, a check that could not run, and a repeat of the
 *  same result are not rows. */
export type BackgroundWatchChangeKind = "baseline" | "failing" | "passing" | "turned_off";

/** What Crew did when that change happened: raised the Needs you row, started the crew's template, resolved the row,
 *  or nothing. */
export type BackgroundWatchChangeAction = "needs_you" | "started_template" | "resolved" | "none";

/** A watch's last check: its state, when it last checked, what it found and the redacted tail it kept. The command is
 *  the crew's own trigger setting and is not repeated here. */
export interface BackgroundWatchStatus {
  crew_id: string;
  state: BackgroundWatchState;
  /** When the last check ran (or could not run), as RFC 3339; absent before the first one. */
  last_checked_at?: string | null;
  /** The exit code of the last check that ran, when it had one. */
  last_exit_code?: number | null;
  /** Why the last check could not run: `timed out`, `could not start`, `refused` or `folder not trusted`. Absent when it ran. */
  reason?: string | null;
  /** Whether a first check has run and set the baseline. A baseline never raises. */
  baseline_done: boolean;
  /** How many checks in a row could not run. Three raise one Needs you row. */
  error_streak: number;
  /** The last check's output, redacted, the last 20 lines and at most 2 KiB. */
  tail?: string | null;
}

/** One row of a watch's change history, with the exit code and redacted tail the check had at that change. */
export interface BackgroundWatchChange {
  id: number;
  kind: BackgroundWatchChangeKind;
  action: BackgroundWatchChangeAction;
  /** When the change happened, as RFC 3339. */
  at: string;
  exit_code?: number | null;
  tail?: string | null;
}

/** A watch's changes, newest first, one page. */
export interface BackgroundWatchChanges {
  changes: BackgroundWatchChange[];
  total: number;
  next_offset?: number | null;
}

/** The two watch kinds of `NeedsYouKind`; every other kind already has a generated type. */
export type WatchNeedsKind = "watch_failing" | "watch_could_not_run";

/** One watch item of `GET /api/crew/needs-you`: the shared `NeedsYouItem` with a watch kind. */
export interface WatchNeedsItem {
  /** The watch crew's id. */
  id: string;
  kind: WatchNeedsKind;
  project_id?: string | null;
  crew_id?: string | null;
  /** When the item began to wait, as RFC 3339. */
  since: string;
  dismissible: boolean;
  /** One redacted line: the core's fixed words, or the streak and why for a check that could not run. */
  reason?: string | null;
}

/** Everything that waits for the person. Only the watch items are read here; the other kinds are already assembled. */
export interface WatchNeedsList {
  items: WatchNeedsItem[];
  count: number;
}
