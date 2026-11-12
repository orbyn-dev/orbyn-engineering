// The decision chain of one request, in the order the drawer reads it: the hard rule (decided by
// code), the lead, then a second key. Pure: the words are the designs' own; the reasons are the row's.
import type { ApprovalRow } from "./model";
import { engineLine } from "./parts";

export type StepTone = "ok" | "no" | "skip" | "am" | "plain";
export interface Step {
  tone: StepTone;
  title: string;
  text: string;
}

const guarded = (row: ApprovalRow) => row.hard === "guard" || row.hard === "guard_two_keys";

function hardStep(row: ApprovalRow): Step {
  if (row.hard === "denied") return { tone: "no", title: "Denied by a hard rule", text: `${row.rule ?? "This"} is always denied.` };
  if (row.hard === "person_only") return { tone: "plain", title: "Person-only action", text: `${row.rule ?? "This"} is something you do yourself.` };
  if (guarded(row)) {
    return {
      tone: "plain",
      title: "Hard rule: the guard",
      text: row.result === "held_second_key" ? "A destructive command against a production-linked target. It needs two keys." : "Two keys are needed for this target.",
    };
  }
  return { tone: "ok", title: "Hard rule", text: row.result === "allowed" ? "Passed. Not a denied kind of action." : "Passed." };
}

function leadStep(row: ApprovalRow): Step | null {
  const lead = row.lead;
  const key = guarded(row) ? "Key 1: " : "";
  if (lead.state === "yes") return { tone: "ok", title: `${key}${key ? "the lead said yes" : "The lead said yes"}`, text: lead.reason };
  if (lead.state === "no") return { tone: "no", title: `${key}${key ? "the lead said no" : "The lead said no"}`, text: lead.reason };
  if (lead.state === "not_asked") {
    const text = row.hard === "person_only" ? "The lead can’t approve it." : row.tier === "always_allowed" ? "This kind of action is always allowed." : "A rule decided. The lead can’t overrule it.";
    return { tone: "skip", title: "Not asked", text };
  }
  return null;
}

function secondStep(row: ApprovalRow): Step | null {
  const second = row.second;
  const guard = guarded(row);
  const label = guard ? "Key 2" : "Second key";
  switch (second.state) {
    case "yes":
    case "no": {
      const who = row.result === "keys_disagree"
        ? `${second.by.engine} · a different vendor: ${second.reason}`
        : second.state === "yes"
          ? `${engineLine(second.by)}, a different vendor from the lead.`
          : `${engineLine(second.by)}, a different vendor: ${second.reason}`;
      return { tone: second.state === "yes" ? "ok" : "no", title: `${label}: ${second.state}`, text: who };
    }
    case "reviewing":
      return { tone: "skip", title: `${label}: waiting`, text: `${engineLine(second.by)}, a different vendor from the lead, is reviewing.` };
    case "unavailable":
      return {
        tone: "skip",
        title: `${label}: not available`,
        text: guard
          ? `A second key needs a different vendor from the lead. ${row.asker?.engine ?? "One engine"} is the one vendor signed in.`
          : `${row.tier === "medium" ? "Medium risk needs" : "This request needs"} an approver on a different vendor, and no second vendor is signed in.`,
      };
    case "not_asked":
      return {
        tone: "skip",
        title: "Not asked",
        text: row.hard === "denied" ? "A second key applies to medium risk." : row.hard === "person_only" ? "A person-only action has no second key." : "No second key after a no.",
      };
    default:
      return null;
  }
}

export function chainOf(row: ApprovalRow): Step[] {
  return [hardStep(row), leadStep(row), secondStep(row)].filter((step): step is Step => step !== null);
}

/** What comes after the steps: the verdict boxes and the closing sentence. */
const NAME = { off: "Off", on: "On", full: "Full" } as const;

/** What comes after the steps: the verdict boxes and the closing sentence, from what the row itself says. */
export function verdictOf(row: ApprovalRow): { steps: Step[]; foot: string | null } {
  switch (row.result) {
    case "allowed":
      // Nobody is asked for an always-allowed action, so nothing is said about the lead or a second key.
      if (row.tier === "always_allowed") return { steps: [], foot: row.note };
      return {
        steps: [],
        foot: `${row.tier === "medium" ? "Medium risk needs both keys. " : ""}The lead saw a structured summary of the action: its kind, target and the brief it serves. It never saw raw page or tool text.`,
      };
    case "keys_disagree":
      return {
        steps: [{ tone: "no", title: "Denied, because the keys disagree", text: "Both keys are needed. When they disagree, the action is denied, the disagreement is logged with both reasons, and nobody is asked to break the tie unless the task needs this action to go on." }],
        foot: null,
      };
    case "person_only":
      return {
        steps: [{ tone: "am", title: "Waiting for your review, not blocking anything", text: "In Autopilot a person-only request is denied and the work continues. It sits in Needs you so you can do it yourself when you choose." }],
        foot: null,
      };
    case "no_second_key": {
      if (guarded(row)) {
        return {
          steps: [{ tone: "no", title: "Denied for now, and you can approve it yourself", text: "Under Full a guarded command needs the lead and a second vendor. With no second vendor signed in it is denied for now and the worker takes another approach. A Needs you item says “Sign in to a second engine to allow this”, and you can approve the command yourself, as one deliberate action that names it. That doesn’t make you a second key: you are the final authority, and it is logged." }],
          foot: null,
        };
      }
      const name = row.mode ? NAME[row.mode] : null;
      const carry = "The worker is told to take another approach, and the rest of the crew carries on.";
      return {
        steps: [
          {
            tone: "no",
            title: name ? `Denied under ${name}, not asked` : "Denied, not asked",
            text: row.mode === "full" ? `Autopilot Full doesn’t stop to ask you. ${carry} Under On or Off, Crew would ask you instead.` : carry,
          },
          ...(row.needsYou ? [{ tone: "plain" as const, title: "A Needs you item, not blocking anything", text: `“${row.needsYou}”. It stays in your Needs you list until you sign in or dismiss it.` }] : []),
        ],
        foot: null,
      };
    }
    case "denied_second_key":
      return {
        steps: [{ tone: "no", title: "Denied, because one key said no", text: `Both keys are needed. ${row.note ?? "The worker was told the second key’s reason."} The decision and both reasons are in the ledger.` }],
        foot: null,
      };
    case "held_second_key":
      return {
        steps: [{ tone: "plain", title: "Why two keys", text: "This command deletes data in a production-linked target. The guard is a hard rule, run by code before any model is asked, in every mode including Full. It can’t be turned off." }],
        foot: null,
      };
    default:
      return { steps: [], foot: row.note };
  }
}
