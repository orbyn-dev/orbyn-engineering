// The one place a ledger row (`AutopilotLogRow`, as the core stores it) becomes the row the log draws.
// The stored row names the task, the action, its mode and tier (`always_allowed`, `low`: the lead decides,
// `medium`: two keys), who decided and the two keys' answers. It does not name the asker's model, the brief or
// the guard, so those stay empty here and the screens leave them out.
import type { AutopilotLogRow } from "../../../../generated/AutopilotLogRow";
import type { AutopilotRule } from "../../../../generated/AutopilotRule";
import { HARD_DENIED } from "./copy";
import type { ApprovalRow, EngineRef, HardKind, LeadAnswer, ResultKind, SecondAnswer, TripwireStop } from "./model";

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

const TIERS = ["always_allowed", "low", "medium"] as const;
const MODES = ["off", "on", "full"] as const;

export function rowOf(row: AutopilotLogRow, rules: AutopilotRule[], roots: string[] = []): ApprovalRow | null {
  if (!APPROVAL_KINDS.includes(row.kind)) return null;
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
    result = "allowed";
    hard = tier === "always_allowed" ? "always_allowed" : "passed";
  } else if (row.kind === "disagreement") {
    result = "keys_disagree";
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
  const title = titleOf(row, roots);
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
    facts: [
      ...(row.action_kind ? [{ label: "Action", text: row.action_kind.replace(/_/g, " ") }] : []),
      ...(row.target ? [{ label: "Target", text: relative(row.target, roots) }] : []),
      { label: "Task", text: row.task_id },
    ],
    block: null,
    note: null,
    needsYou: row.needs_second_engine ? `Sign in to a second engine to allow: ${lower(title)}` : null,
    mode,
    tier,
    hard,
    rule: rule ? sentence(rule) : null,
    lead,
    leadBy: null,
    second,
    result,
    decidedInMs: null,
    guard: null,
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
