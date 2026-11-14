// What the map and the lanes show from the routes' answers: the words and tones of Crew Designs and of the service's own wire values.
// Pure functions. There is no word here for what the routes do not carry (a worker's tokens, a branch, a level, a blocked state, a
// message's text, a fact's value).
import type { FlowDeliveryMode } from "../../../generated/FlowDeliveryMode";
import type { FlowEngineStatus } from "../../../generated/FlowEngineStatus";
import type { FlowLane } from "../../../generated/FlowLane";
import type { FlowNode } from "../../../generated/FlowNode";
import type { FlowQuiet } from "../../../generated/FlowQuiet";
import type { FlowSubagents } from "../../../generated/FlowSubagents";

export type Tone = "neutral" | "info" | "warn" | "err" | "ok";

/** The state a node shows: the task's, or "Lead" for the lead (it has no task). Needs you is the only amber; Failed is red. */
export function stateWord(node: Pick<FlowNode, "role" | "state">): { text: string; tone: Tone } {
  if (node.role === "lead" || !node.state) return { text: "Lead", tone: "neutral" };
  switch (node.state) {
    case "briefed": return { text: "Queued", tone: "neutral" };
    case "routed": return { text: "Routed", tone: "neutral" };
    case "running":
    case "changed_approach": return { text: "Running", tone: "info" };
    case "verifying": return { text: "Verifying", tone: "info" };
    case "needs_person": return { text: "Needs you", tone: "warn" };
    case "failed": return { text: "Failed", tone: "err" };
    case "passed": return { text: "Passed", tone: "ok" };
    case "stopped": return { text: "Stopped", tone: "neutral" };
    case "cancelled": return { text: "Cancelled", tone: "neutral" };
    default: return { text: node.state.replaceAll("_", " "), tone: "neutral" };
  }
}

/** How long a running worker has been silent. Active says nothing; quiet is quiet ink, not amber; may be stuck is the one that asks. */
export function quietWord(quiet: FlowQuiet): { text: string; tone: Tone } | null {
  if (quiet === "quiet") return { text: "Quiet · 5 min", tone: "neutral" };
  if (quiet === "may_be_stuck") return { text: "May be stuck", tone: "warn" };
  return null;
}

/** What the registry knows of the engine. */
export function engineStatusWord(status: FlowEngineStatus): string {
  switch (status) {
    case "ready": return "Ready";
    case "not_installed": return "Not installed";
    case "signed_out": return "Signed out";
    case "sign_in_unconfirmed": return "Sign-in unconfirmed";
    case "broken": return "Broken";
  }
}

/**
 * The line a crew shows above its workers for an engine that cannot take work. There is no offline value on the wire: the offline
 * frame is drawn from `not_installed` and the signed-out frame from `signed_out`. The other two have no frame, and say what the
 * registry says, in its words: the same word as the row's chip.
 */
export function engineProblem(engine: string, status: FlowEngineStatus): string | null {
  switch (status) {
    case "ready": return null;
    case "not_installed": return `${engine} isn’t running.`;
    case "signed_out": return `${engine} isn’t signed in.`;
    // The two states no frame draws say the registry's own word, the same one the row's chip carries.
    case "sign_in_unconfirmed": return `${engine}: ${engineStatusWord(status)}`;
    case "broken": return `${engine}: ${engineStatusWord(status)}`;
  }
}

/** Only an engine that says so shows its subagents; the others say they are not reported (never drawn as an empty tree). */
export const subagentsWord = (subagents: FlowSubagents): string | null => (subagents === "not_reported" ? "Subagents not reported" : null);

/** How a message arrived. No delivery row, or recipients that disagree, is no word at all: never guessed. */
export function deliveryWord(delivery: FlowDeliveryMode | null | undefined): string | null {
  switch (delivery) {
    case "live": return "live";
    case "next_turn": return "next turn";
    case "tool_result": return "tool result";
    case "held": return "held";
    case "failed": return "failed";
    default: return null;
  }
}

const KINDS: Record<string, string> = { fact: "a fact", question: "a question", handoff: "a handoff", blocker: "a blocker", answer: "an answer" };
/** The label of a message kind: the service's own, else the fixed one for the kind, else "a message". */
export const kindLabel = (kind: string, label?: string | null): string => (label && label.trim() !== "" ? label : (KINDS[kind] ?? "a message"));

/** The lease line: the patterns the worker may change, or "read only" when it shares the lead's checkout. */
export const leaseWord = (node: Pick<FlowNode, "lease" | "read_only">): string => (node.read_only ? "read only" : node.lease.length > 0 ? node.lease.join(", ") : "no files");

/** The model line: model, then effort. Empty when neither is known. */
export const modelWord = (node: Pick<FlowNode, "model" | "effort">): string => [node.model, node.effort ? node.effort[0]!.toUpperCase() + node.effort.slice(1) : null].filter(Boolean).join(" · ");

/** The lead first, then the workers in the order given. A worker's parent is always the lead: depth is one. */
export function ordered(nodes: FlowNode[]): FlowNode[] {
  return [...nodes.filter((node) => node.role === "lead"), ...nodes.filter((node) => node.role !== "lead")];
}

/** The time of a message as the clock reads it, "14:09". An unreadable time is shown as sent. */
export function clock(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : at.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

/** A finding's check, in Crew Designs' words: unchecked and pending are both "Not checked yet". */
export function checkWord(check: string): string {
  switch (check) {
    case "confirmed": return "Confirmed";
    case "unconfirmed": return "Unconfirmed";
    case "pending":
    case "unchecked": return "Not checked yet";
    default: return check.replaceAll("_", " ");
  }
}

/** A finding's status against the lane's later rounds. */
export function findingStatusWord(status: string): string {
  switch (status) {
    case "open": return "Open";
    case "fixed": return "Fixed";
    case "regressed": return "Regressed";
    case "superseded": return "Superseded";
    case "unreported": return "Not reported since";
    default: return status.replaceAll("_", " ");
  }
}

/** What a reviewer lane says about how it ran: the tier, the commit it read, and whether it ran on the engine that built the work. */
export function laneFacts(lane: Pick<FlowLane, "tier" | "commit" | "same_engine">): string[] {
  return [
    lane.tier ? `${lane.tier} tier` : null,
    lane.commit ? `read ${lane.commit.slice(0, 7)}` : null,
    lane.same_engine === true ? "Same engine as author" : lane.same_engine === false ? "Different engine from the author" : null,
  ].filter((one): one is string => one !== null);
}
