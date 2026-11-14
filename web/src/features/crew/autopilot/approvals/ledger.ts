// The one place a ledger row (`AutopilotLogRow`, as the core stores it) becomes the row the log draws.
// The stored row names the task, the action, its mode and tier (`always_allowed`, `low`: the lead decides,
// `medium`: two keys), who decided and the two keys' answers. It does not name the asker's model, the brief or
// the guard, so those stay empty here and the screens leave them out.
import type { AutopilotLogRow } from "../../../../generated/AutopilotLogRow";
import type { AutopilotRule } from "../../../../generated/AutopilotRule";
import { HARD_DENIED } from "./copy";
import { titleOfCommand } from "./held";
import type { ApprovalRow, EngineRef, GuardFacts, HardKind, LeadAnswer, ResultKind, SecondAnswer, TripwireStop } from "./model";

/** The kinds the approval log lists. The ledger also holds merge rows and trip wires. */
export const APPROVAL_KINDS = ["allowed", "denied", "person", "disagreement"];

const clock = (iso: string) => {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
};

const WEEKDAY = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** "Sat 7 Nov", with the year when asked. */
export function dayOf(iso: string, year = false): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  return `${WEEKDAY[at.getDay()]} ${at.getDate()} ${MONTH[at.getMonth()]}${year ? ` ${at.getFullYear()}` : ""}`;
}

/** "Today · Sat 7 Nov 2026" for the day the newest row is from. */
export function dayLine(iso: string | undefined, now = new Date()): string | null {
  if (!iso) return null;
  const at = new Date(iso);
  const today = at.toDateString() === now.toDateString();
  return `${today ? "Today · " : ""}${dayOf(iso, true)}`;
}

const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
const lower = (text: string) => text.charAt(0).toLowerCase() + text.slice(1);

/** An engine id as the ledger gives it, with no model. */
const engineOnly = (engine: string): EngineRef => ({ engine, model: "", vendor: "" });

/** A rule in the designs' words when they name it, else the core's own. */
export function ruleName(id: string | null | undefined, rules: AutopilotRule[]): string | null {
  if (!id) return null;
  return HARD_DENIED.find(([known]) => known === id)?.[1] ?? rules.find((one) => one.id === id)?.rule ?? id;
}

function titleOf(row: AutopilotLogRow, roots: string[]): string {
  const target = relative(row.target?.trim() || row.paths[0] || "", roots);
  const kind = row.action_kind ?? "";
  if (target && kind) return `${sentence(kind.replace(/_/g, " "))} ${target}`;
  return target || sentence(kind.replace(/_/g, " ")) || `Request ${row.id}`;
}

/** A path as the repo reads it: the checkout's own prefix is dropped when one is known. */
export function relative(path: string, roots: string[]): string {
  const root = roots.filter((one) => one && path.startsWith(`${one.replace(/\/$/, "")}/`)).sort((a, b) => b.length - a.length)[0];
  return (root ? path.slice(root.replace(/\/$/, "").length + 1) : path).replace(/^\.\//, "");
}

/** What a guarded command's ledger rows carry in `body.guard`: the category, the redacted target, what matched and which list forced it. */
export interface GuardBody {
  category: string;
  target: string;
  matchedBy: string | null;
  /** `prod` when this device's prod list forced it, else `none`. */
  list: string | null;
}

const text = (value: unknown): string | null => (typeof value === "string" && value !== "" ? value : null);

export function guardBody(row: AutopilotLogRow): GuardBody | null {
  const body = row.body as { guard?: { category?: unknown; target?: unknown; matched_by?: unknown; list?: unknown } } | null;
  const guard = body?.guard;
  if (!guard || typeof guard !== "object") return null;
  const target = text(guard.target);
  return target ? { category: text(guard.category) ?? "", target, matchedBy: text(guard.matched_by), list: text(guard.list) } : null;
}

/** The exit status of a command the guard let through, by the id of the ledger row that let it through. `unknown` is what the ledger says when none came. */
export type Exit = number | "unknown";

/** What the pages read so far know about other rows: exit statuses, and the guard facts of each hold. */
export interface Side {
  exits: ReadonlyMap<number, Exit>;
  holds: ReadonlyMap<string, { guard: GuardBody; at: string }>;
}

/** Reads the rows a page adds to `side`: a `command_exit` row (`body.row` is the row that let the command through) and a `held` row (`body.hold`). */
export function learn(rows: AutopilotLogRow[], exits: Map<number, Exit>, holds: Map<string, { guard: GuardBody; at: string }>): void {
  for (const row of rows) {
    const body = (row.body ?? {}) as { row?: unknown; hold?: unknown; exit_code?: unknown };
    if (row.kind === "command_exit" && typeof body.row === "number") {
      exits.set(body.row, typeof body.exit_code === "number" ? body.exit_code : "unknown");
    } else if (row.kind === "held" && typeof body.hold === "string") {
      const guard = guardBody(row);
      if (guard) holds.set(body.hold, { guard, at: row.at });
    }
  }
}

/** The facts the card shows of a guarded command. A target that is a variable or cannot be read says so in the service's own words. */
export function guardFacts(guard: GuardBody, command: string, at: string, exit: Exit | undefined): GuardFacts {
  return {
    short: titleOfCommand(command),
    target: guard.target,
    kind: guard.category === "database" ? "remote database" : "production-linked",
    markedBy: guard.list === "prod" ? "prod_targets" : null,
    reason: "This command deletes data in a production-linked target.",
    at: clock(at) || null,
    exit: typeof exit === "number" ? exit : null,
    exitUnknown: exit === "unknown",
  };
}

const TIERS = ["always_allowed", "low", "medium"] as const;
const MODES = ["off", "on", "full"] as const;

export function rowOf(row: AutopilotLogRow, rules: AutopilotRule[], roots: string[] = [], side?: Side): ApprovalRow | null {
  if (!APPROVAL_KINDS.includes(row.kind)) return null;
  const guard = guardBody(row);
  // Under On and Off the guard denies and holds: the hold is the request, and it is drawn from the held list.
  if (guard && row.kind === "denied" && row.decided_by === "guard") return null;
  // A disagreement is stored twice: as its own row and as the denial it caused. The log lists it once.
  if (row.kind === "denied" && row.lead?.approve === true && row.second?.approve === false) return null;
  const tier = TIERS.find((one) => one === row.tier) ?? null;
  const mode = MODES.find((one) => one === row.mode) ?? null;
  const hardRule = row.decided_by === "hard_rule" || (row.kind === "denied" && row.rule != null && row.decided_by == null);
  const rule = ruleName(row.rule, rules);
  const lead: LeadAnswer =
    row.lead != null
      ? { state: row.lead.approve ? "yes" : "no", reason: sentence(row.lead.reason), short: lower(row.lead.reason).replace(/\.$/, "") }
      : { state: "not_asked" };
  const second: SecondAnswer = row.second
    ? { state: row.second.approve ? "yes" : "no", by: engineOnly(row.second.engine), reason: lower(row.second.reason) }
    : row.needs_second_engine
      ? { state: "unavailable" }
      : row.kind === "allowed" && tier === "always_allowed"
        ? { state: "not_needed" }
        : { state: "not_asked" };
  let result: ResultKind;
  let hard: HardKind = "passed";
  if (row.kind === "allowed") {
    // A guarded command that ran: the person approved it once, or both keys said yes.
    result = guard ? (row.decided_by === "person" ? "approved_you" : "approved_two_keys") : "allowed";
    hard = tier === "always_allowed" ? "always_allowed" : "passed";
  } else if (row.kind === "disagreement") {
    result = guard ? "denied_second_key" : "keys_disagree";
  } else if (row.kind === "person") {
    result = "person_only";
    hard = "person_only";
  } else if (hardRule) {
    result = "denied_rule";
    hard = "denied";
  } else if (row.needs_second_engine) {
    result = "no_second_key";
  } else {
    result = "denied_lead";
  }
  if (guard) hard = row.decided_by === "person" ? "guard" : "guard_two_keys";
  const person = guard !== null && result === "approved_you";
  const title = guard && row.target ? titleOfCommand(row.target) : titleOf(row, roots);
  return {
    id: String(row.id),
    time: clock(row.at),
    at: row.at,
    title,
    asker: null,
    taskId: row.task_id,
    branch: null,
    risk: tier === "low" ? "low" : tier === "medium" ? "medium" : null,
    notes: [],
    facts: guard ? [] : [
      ...(row.action_kind ? [{ label: "Action", text: row.action_kind.replace(/_/g, " ") }] : []),
      ...(row.target ? [{ label: "Target", text: relative(row.target, roots) }] : []),
      { label: "Task", text: row.task_id },
    ],
    block: guard ? (row.target ?? null) : null,
    note: null,
    needsYou: row.needs_second_engine ? `Sign in to a second engine to allow: ${lower(title)}` : null,
    mode,
    tier,
    hard,
    rule: rule ? sentence(rule) : null,
    lead,
    leadBy: null,
    second: person ? { state: "you" } : second,
    result,
    decidedInMs: null,
    guard: guard ? guardFacts(guard, row.target ?? "", row.at, result === "approved_you" || result === "approved_two_keys" ? side?.exits.get(row.id) : undefined) : null,
  };
}

/** What stopped a task: the trip-wire row's own body names the wire. */
export function stopOf(row: AutopilotLogRow, limits: { inARow: number; inARun: number }): TripwireStop {
  const body = (row.body ?? {}) as { wire?: string; in_a_row?: number; in_a_run?: number };
  const injected = body.wire === "injection";
  const inARow = body.in_a_row ?? 0;
  const inARun = body.in_a_run ?? 0;
  const byRun = !injected && inARow < limits.inARow && inARun >= limits.inARun;
  return {
    taskId: row.task_id,
    taskTitle: row.task_id,
    engine: null,
    reason: injected ? "injected_instructions" : "too_many_denials",
    denialCount: byRun ? inARun : inARow,
    inARun: byRun,
    denials: [],
    // The ledger keeps the class of text that looked like instructions, never the text.
    injected: null,
  };
}
