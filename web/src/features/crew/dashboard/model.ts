// What the Crew dashboard says, worked out from the reads. Pure: no fetch, no React. Every number from the service is checked before it is shown
// (a negative, fractional or missing one reads as "not measured", never as zero), every name is capped, and a service-supplied word is looked up in
// a Map, never used as an object key.
import type { AutopilotTrust } from "../../../generated/AutopilotTrust";
import { PERIODS, type DashboardHistory, type HistoryMerge, type Period } from "./contract";

export const MAX_TEXT = 60;
export const MAX_ROWS = 50;
export const MAX_WEEKS = 14;
export const MAX_LIST = 20;
/** Used only while a service answer carries no minimum of its own. */
const RATE_SAMPLE = 10;
const MEDIAN_SAMPLE = 5;

/** A whole, non-negative count, or `null`: anything else (a string, NaN, a negative, a fraction) is not a count. */
export function whole(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}
const count = (value: unknown) => whole(value) ?? 0;
const orNull = (value: unknown) => (typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null);

/** Text from outside, capped. A control character is a space, and the cut says so with an ellipsis. */
export function text(value: unknown, max = MAX_TEXT): string {
  if (typeof value !== "string") return "";
  let clean = "";
  for (const char of value) {
    const code = char.codePointAt(0) ?? 0;
    clean += code < 0x20 || (code >= 0x7f && code <= 0x9f) || code === 0x2028 || code === 0x2029 ? " " : char;
  }
  clean = clean.replace(/\s+/g, " ").trim();
  return clean.length > max ? `${clean.slice(0, max - 1)}…` : clean;
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`;
export const mergesOf = (n: number) => plural(n, "merge");

/** A whole percent of a part in a whole, or `null` when there is nothing to divide by. */
export function percent(part: number, whole_: number): string | null {
  return whole_ > 0 ? `${Math.round((part / whole_) * 100)}%` : null;
}

/** 6_100_000 → "6.1M", 540_000 → "540k", 812 → "812". */
export function compact(amount: number): string {
  if (amount >= 1_000_000) return `${trim(amount / 1_000_000)}M`;
  if (amount >= 1_000) return `${trim(amount / 1_000)}k`;
  return Math.round(amount).toLocaleString("en-US");
}
const trim = (n: number) => (n >= 100 ? String(Math.round(n)) : String(Math.round(n * 10) / 10));

const UNITS = new Map<string, string>([
  ["tokens", "tokens"],
  ["credits", "credits"],
  ["requests", "requests"],
]);
/** A cost in its own unit: "6.1M tokens". An unknown unit is shown as given, capped. */
export const costText = (cost: { unit: string; amount: number }) => `${compact(cost.amount)} ${UNITS.get(cost.unit) ?? (text(cost.unit, 20) || "units")}`;

/** "41 min", "2 h 05 min", "under 1 min". */
export function duration(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return "under 1 min";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${String(rest).padStart(2, "0")} min`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/** "6 Nov" in the viewer's zone; an unusable time is "an unknown date". */
export function day(unix: number): string {
  const at = new Date(unix * 1000);
  return Number.isNaN(at.getTime()) ? "an unknown date" : `${at.getDate()} ${MONTHS[at.getMonth()]}`;
}
/** "Fri 6 Nov". */
export function dayLong(unix: number): string {
  const at = new Date(unix * 1000);
  return Number.isNaN(at.getTime()) ? "an unknown date" : `${WEEKDAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]}`;
}

export interface EngineRow {
  name: string;
  model: string | null;
  merges: number;
  reverted: number;
  /** `null`: the read that carries it is not served. */
  unaided: number | null;
  cost: { unit: string; amount: number } | null;
}
export interface AreaRow {
  name: string;
  merges: number;
  reverted: number;
  unaided: number | null;
}
export interface WeekRow {
  start: number;
  merges: number;
  unaided: number;
  reverted: number;
}
export interface RevertRow {
  title: string;
  at: number;
  by: "person" | "auto" | "unknown";
  certain: boolean | null;
}
export interface Cycle {
  median: number | null;
  p90: number | null;
  fastest: number | null;
  slowest: number | null;
}

/** One kind of wait that is in the inbox now, and how many of it. */
export interface LiveWait {
  kind: string;
  count: number;
}
/** One reason workers own live work now, and how many of them. */
export interface LiveWork {
  why: string;
  count: number;
}
/** The dashboard's "Right now" row (KC3): the live read as the page draws it, in the order the service gave. */
export interface DashLive {
  needsYou: LiveWait[];
  working: LiveWork[];
  truncated: boolean;
}

export interface DashModel {
  /** Where the figures come from: the history read, or the trust record alone (which has no autonomy, cycle time, cost or weeks). */
  source: "history" | "trust";
  days: number;
  merges: number;
  reverted: number;
  /** A rate shows from this many merges; a median from `minMedian`. */
  minRate: number;
  minMedian: number;
  /** Absent from the trust record: the history read is what carries it. */
  unaided: number | null;
  revertedAuto: number | null;
  revertedByPerson: number | null;
  cycle: Cycle | null;
  weeks: WeekRow[] | null;
  engines: EngineRow[];
  areas: AreaRow[];
  reverts: RevertRow[] | null;
  recent: HistoryMerge[] | null;
  /** The live row. `null` when the read carries none (an older service) or the trust record answers. */
  live: DashLive | null;
  truncated: boolean;
}

/** Every kind of wait the service can send, in the page's own words. */
const WAITING_KIND_LABELS = new Map<string, string>([
  ["task_needs_person", "Asked a question"],
  ["merge_waiting", "A merge that waits"],
  ["merge_gate_waiting", "A merge to release"],
  ["command_approval", "A command to approve"],
  ["manual_check", "A check only you can make"],
  ["could_not_start", "A task that could not start"],
  ["could_not_stop", "A task that could not be stopped"],
  ["tripwire_stop", "A task stopped by a trip wire"],
  ["worker_stopped_twice", "A worker that stopped twice"],
  ["watch_failing", "A watch that is failing"],
  ["watch_could_not_run", "A watch that could not run"],
  ["background_review", "A background run to review"],
]);
/** A kind this page does not know is never dropped: its raw word is drawn with underscores as spaces. */
export function waitingKindLabel(kind: string): string {
  const raw = text(kind).replace(/_/g, " ").replace(/\s+/g, " ").trim();
  return WAITING_KIND_LABELS.get(kind) ?? (raw || "unknown");
}

/** Every reason workers own live work now, in the page's own words. */
const WORKING_WHY_LABELS = new Map<string, string>([
  ["running", "Running"],
  ["verifying", "Checks running"],
  ["retry", "Retrying after a failed check"],
  ["fix", "Fixing a review finding"],
]);
/** A reason this page does not know is drawn as the raw word. */
export function workingWhyLabel(why: string): string {
  return WORKING_WHY_LABELS.get(why) ?? (text(why) || "unknown");
}

const liveOf = (live: DashboardHistory["live"]): DashLive | null => {
  if (!live || typeof live !== "object" || Array.isArray(live)) return null;
  const waits = Array.isArray(live.needs_you) ? live.needs_you : [];
  const works = Array.isArray(live.working) ? live.working : [];
  return {
    needsYou: waits
      .map((row) => ({ kind: typeof row?.kind === "string" ? row.kind : "", count: count(row?.count) }))
      .filter((row) => row.count > 0),
    working: works
      .map((row) => ({ why: typeof row?.why === "string" ? row.why : "", count: count(row?.count) }))
      .filter((row) => row.count > 0),
    truncated: live.truncated === true,
  };
};

const nameOf = (value: unknown, fallback = "unknown") => text(value) || fallback;

export function fromTrust(trust: AutopilotTrust): DashModel {
  const total = (trust as Partial<AutopilotTrust>).total;
  const rows = <T extends { name: string; rate: { merges: number; reverts: number } }>(list: unknown): T[] =>
    (Array.isArray(list) ? (list as T[]) : []).slice(0, MAX_ROWS);
  return {
    source: "trust",
    days: whole(trust.window_days) ?? 0,
    merges: count(total?.merges),
    reverted: count(total?.reverts),
    minRate: whole(trust.min_sample) ?? RATE_SAMPLE,
    minMedian: MEDIAN_SAMPLE,
    unaided: null,
    revertedAuto: null,
    revertedByPerson: null,
    cycle: null,
    weeks: null,
    engines: rows(trust.engines).map((row) => ({ name: nameOf(row?.name), model: null, merges: count(row?.rate?.merges), reverted: count(row?.rate?.reverts), unaided: null, cost: null })),
    areas: rows(trust.areas).map((row) => ({ name: nameOf(row?.name), merges: count(row?.rate?.merges), reverted: count(row?.rate?.reverts), unaided: null })),
    reverts: null,
    recent: null,
    live: null,
    truncated: trust.truncated === true,
  };
}

const BY = new Map<unknown, RevertRow["by"]>([["person", "person"], ["auto", "auto"]]);
const NEEDED = new Map<unknown, HistoryMerge["needed"]>([["nothing", "nothing"], ["answer", "answer"], ["approval", "approval"], ["other", "other"]]);
const CHECKS = new Map<unknown, HistoryMerge["checks"]>([["passed", "passed"], ["failed", "failed"], ["none", "none"]]);

/** The period an answer names, when it is one the page offers. */
export const isPeriod = (value: unknown): value is Period => PERIODS.some((period) => period === value);

export function fromHistory(history: DashboardHistory, asked: Period): DashModel {
  const list = <T,>(value: unknown, cap: number): T[] => (Array.isArray(value) ? (value as T[]).slice(0, cap) : []);
  const merges = count(history.merges);
  const cycle = history.cycle && typeof history.cycle === "object"
    ? { median: orNull(history.cycle.median_s), p90: orNull(history.cycle.p90_s), fastest: orNull(history.cycle.fastest_s), slowest: orNull(history.cycle.slowest_s) }
    : null;
  return {
    source: "history",
    days: isPeriod(history.days) ? history.days : asked,
    merges,
    reverted: count(history.reverted),
    minRate: whole(history.min_rate_sample) || RATE_SAMPLE,
    minMedian: whole(history.min_median_sample) || MEDIAN_SAMPLE,
    unaided: whole(history.unaided),
    revertedAuto: whole(history.reverted_auto),
    revertedByPerson: whole(history.reverted_by_person),
    cycle,
    weeks: list<DashboardHistory["weeks"][number]>(history.weeks, MAX_WEEKS).map((week) => ({ start: count(week?.start), merges: count(week?.merges), unaided: count(week?.unaided), reverted: count(week?.reverted) })),
    engines: list<DashboardHistory["engines"][number]>(history.engines, MAX_ROWS).map((row) => ({
      name: nameOf(row?.name),
      model: text(row?.model) || null,
      merges: count(row?.merges),
      reverted: count(row?.reverted),
      unaided: whole(row?.unaided),
      cost: row?.cost && typeof row.cost.unit === "string" && orNull(row.cost.amount) !== null ? { unit: row.cost.unit, amount: row.cost.amount } : null,
    })),
    areas: list<DashboardHistory["areas"][number]>(history.areas, MAX_ROWS).map((row) => ({ name: nameOf(row?.name), merges: count(row?.merges), reverted: count(row?.reverted), unaided: whole(row?.unaided) })),
    reverts: list<DashboardHistory["reverts"][number]>(history.reverts, MAX_LIST).map((row) => ({ title: nameOf(row?.title, "A merge"), at: count(row?.at), by: BY.get(row?.by) ?? "unknown", certain: typeof row?.certain === "boolean" ? row.certain : null })),
    recent: list<HistoryMerge>(history.recent, MAX_LIST).map((row) => ({ title: nameOf(row?.title, "A merge"), engine: nameOf(row?.engine), at: count(row?.at), needed: NEEDED.get(row?.needed) ?? "other", checks: CHECKS.get(row?.checks) ?? "none" })),
    live: liveOf(history.live),
    truncated: history.truncated === true,
  };
}

/** What a share of merges reads as: a percent from the minimum sample, else the merges themselves and why there is no rate. */
export interface Rate {
  /** "71%", or `null` below the minimum. */
  pct: string | null;
  /** "27 of 38". */
  of: string;
  /** "too few for a rate", only when below the minimum. */
  few: string | null;
}
export function rate(part: number, merges: number, minRate: number): Rate {
  const enough = merges >= minRate && merges > 0;
  return { pct: enough ? percent(part, merges) : null, of: `${part.toLocaleString("en-US")} of ${merges.toLocaleString("en-US")}`, few: enough ? null : "too few for a rate" };
}

/** A median needs `minMedian` merges, and a cycle time with no median says "Not measured", never zero. */
export function medianOf(model: DashModel): number | null {
  return model.merges >= model.minMedian ? (model.cycle?.median ?? null) : null;
}

export interface Cost {
  engine: string;
  unit: string;
  amount: number;
}

/**
 * Each engine's cost in its own unit, summed across repositories one engine and one unit at a time. A total across units would mean nothing, so two
 * units of one engine stay two rows, and no figure ever joins engines.
 */
export function costTotals(models: readonly DashModel[]): Cost[] {
  const sums = new Map<string, Cost>();
  for (const model of models) {
    for (const row of model.engines) {
      if (!row.cost) continue;
      const key = `${row.name}\u0000${row.cost.unit}`;
      const seen = sums.get(key);
      if (seen) seen.amount += row.cost.amount;
      else sums.set(key, { engine: row.name, unit: row.cost.unit, amount: row.cost.amount });
    }
  }
  return [...sums.values()];
}

/** The engines of one model that report a cost, in the order they came. */
export const costsOf = (model: DashModel): Cost[] => costTotals([model]);

/** The engine names a repository used, joined and capped: "claude, codex, opencode". */
export const enginesLine = (model: DashModel) => model.engines.map((row) => row.name).slice(0, 6).join(", ");

/** Merges per week: the tallest bar is the busiest week, each bar at least 3 percent so a one-merge week is visible. */
export function barHeights(weeks: readonly WeekRow[]): number[] {
  const tallest = Math.max(1, ...weeks.map((week) => week.merges));
  return weeks.map((week) => (week.merges === 0 ? 0 : Math.max(6, Math.round((week.merges / tallest) * 100))));
}

/** The filled part of a bar, as a share of that bar's own height. */
export const fill = (part: number, merges: number) => (merges > 0 ? Math.min(100, Math.round((part / merges) * 100)) : 0);

/** What a recent merge needed from the person. */
export const NEEDED_TEXT = new Map<HistoryMerge["needed"], string>([
  ["nothing", "merged without you"],
  ["answer", "you answered a question"],
  ["approval", "you approved the request"],
  ["other", "you took part"],
]);
export const CHECKS_TEXT = new Map<HistoryMerge["checks"], string>([["passed", "Passed"], ["failed", "Failed"], ["none", "No checks"]]);
