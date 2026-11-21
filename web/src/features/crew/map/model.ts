// What the map and the lanes show from the routes' answers: the words and tones of Crew Designs and of the service's own wire values.
// Pure functions. There is no word here for what the routes do not carry (a worker's tokens, a branch, a level, a blocked state, a
// message's text, a fact's value).
import { validTaskId } from "../writes/api";
import type { FlowEdge } from "../../../generated/FlowEdge";
import type { FlowMessage } from "../../../generated/FlowMessage";
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

/**
 * Whether a member's name can be sent as the messages filter. The service refuses a name that fails its id check, and a name it
 * scrubbed ("[redacted]", "Withheld.") is no member's id at all: those are shown, but never sent.
 */
export const memberFilterable = (member: string): boolean => validTaskId(member) && member !== "Withheld.";

/** How long a node has been going, as the design writes it ("4m 12s", "2h 14m"); null when the time cannot be read. */
export function since(iso: string | null | undefined, now: number = Date.now()): string | null {
  if (!iso) return null;
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return null;
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 60) return seconds + "s";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return seconds % 60 ? minutes + "m " + (seconds % 60) + "s" : minutes + "m";
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return minutes % 60 ? hours + "h " + (minutes % 60) + "m" : hours + "h";
  const days = Math.floor(hours / 24);
  return hours % 24 ? days + "d " + (hours % 24) + "h" : days + "d";
}

/** A task state that has ended: nothing more happens to it, so its clock stops. */
export const endedState = (state: string | null | undefined): boolean => state === "passed" || state === "failed" || state === "stopped" || state === "cancelled";

/**
 * How long a worker has been going: from its start to now while it runs, and from its start to its last update once its task has
 * ended, so a finished worker's time stops growing. An ended worker with no readable last update has no time, never a running one.
 */
export function elapsedWord(node: Pick<FlowNode, "state" | "created_at" | "updated_at">, now: number = Date.now()): string | null {
  if (!endedState(node.state)) return since(node.created_at, now);
  const end = Date.parse(node.updated_at ?? "");
  return Number.isNaN(end) ? null : since(node.created_at, end);
}

/** The name a lane or a tree card calls a member: the lead is "Lead"; every other member is its own name. */
export const memberName = (node: Pick<FlowNode, "role" | "member">): string => (node.role === "lead" ? "Lead" : node.member);

// The wide tree of Crew Designs' crew-map: the lead in the first column, the workers in the second, each worker's subagent
// report in the third. The widths, heights and gaps are the frame's own values.
const LEAD_W = 232;
const CARD_W = 264;
const CARD_H = 130;
const ROW_GAP = 30;
const WORKER_X = 290;
const SUB_X = 596;
const SUB_W = 236;
const SUB_H = 56;
const START_MID = (LEAD_W + WORKER_X) / 2;

export interface MapCard {
  node: FlowNode;
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface MapSub {
  node: FlowNode;
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface MapTalk {
  /** The dashed line's path. */
  d: string;
  from: string;
  to: string;
  /** The message's label from the read ("a fact"), never its text. */
  label: string;
  delivery: string | null;
  /** Where the label's pill sits, at the middle of the line. */
  x: number;
  y: number;
  /** A cross-column label is centred on the middle of the line; a same-column one starts at the line. */
  center: boolean;
  /** What the pill says to a screen reader, including the edge's own count. */
  title: string;
}
export interface MapTreeView {
  width: number;
  height: number;
  cards: MapCard[];
  subs: MapSub[];
  /** Solid lines, from the lead to each worker whose node says the lead started it. */
  starts: { d: string }[];
  /** Dashed lines, one per message between members, with its label. */
  talks: MapTalk[];
}

const edgeTitle = (from: string, to: string, count: number, label: string, delivery: string | null): string =>
  from + " → " + to + ": " + count + " " + (count === 1 ? "message" : "messages") + ", " + label + (delivery ? ", newest " + delivery : "");

/**
 * The wide tree's positions and lines. The lead sits at the left, the workers stack in one column and each worker's subagent
 * report sits beside it; a worker's parent is the lead (the read's depth is one), so the only start lines are the lead's. A
 * message whose member the map does not carry is not drawn, and neither is one a member sent itself.
 */
export function treeLayout(nodes: FlowNode[], edges: FlowEdge[]): MapTreeView {
  const lead = nodes.find((node) => node.role === "lead") ?? null;
  const workers = nodes.filter((node) => node.role !== "lead");
  const contentH = workers.length === 0 ? 0 : (workers.length - 1) * (CARD_H + ROW_GAP) + CARD_H;
  const leadY = Math.max(0, (contentH - CARD_H) / 2);
  const cards: MapCard[] = [];
  if (lead) cards.push({ node: lead, x: 0, y: leadY, w: LEAD_W, h: CARD_H });
  workers.forEach((node, at) => cards.push({ node, x: WORKER_X, y: at * (CARD_H + ROW_GAP), w: CARD_W, h: CARD_H }));
  const subs: MapSub[] = workers.map((node, at) => ({ node, x: SUB_X, y: at * (CARD_H + ROW_GAP), w: SUB_W, h: SUB_H }));
  const byMember = new Map(cards.map((card) => [card.node.member, card]));
  const starts: { d: string }[] = [];
  for (const card of cards) {
    if (card.node.role === "lead" || !card.node.started_by || !lead) continue;
    starts.push({ d: "M" + LEAD_W + "," + (leadY + CARD_H / 2) + " H" + START_MID + " V" + (card.y + CARD_H / 2) + " H" + WORKER_X });
  }
  const talks: MapTalk[] = [];
  const line = (from: MapCard, to: MapCard): { d: string; x: number; y: number; center: boolean } => {
    const fromY = from.y + from.h / 2;
    const toY = to.y + to.h / 2;
    if (from.x === to.x) {
      const x = from.x + 10;
      return { d: "M" + x + "," + (from.y + from.h) + " V" + to.y, x: x + 12, y: (from.y + from.h + to.y) / 2, center: false };
    }
    const mid = from.x < to.x ? (from.x + from.w + to.x) / 2 : (to.x + to.w + from.x) / 2;
    const startX = from.x < to.x ? from.x + from.w : from.x;
    const endX = from.x < to.x ? to.x : to.x + to.w;
    return { d: "M" + startX + "," + fromY + " H" + mid + " V" + toY + " H" + endX, x: mid, y: (fromY + toY) / 2, center: true };
  };
  for (const edge of edges) {
    const from = byMember.get(edge.from);
    if (!from) continue;
    const delivery = deliveryWord(edge.delivery);
    const targets = edge.to === "all" ? [...cards].filter((card) => card.node !== from.node) : [byMember.get(edge.to)].filter((card): card is MapCard => card !== undefined);
    let labelled = false;
    for (const to of targets) {
      if (to.node === from.node) continue;
      const at = line(from, to);
      talks.push({
        ...at,
        from: edge.from,
        to: edge.to,
        label: labelled ? "" : edge.label,
        delivery,
        title: edgeTitle(edge.from, edge.to, edge.count, edge.label, delivery),
      });
      labelled = true;
    }
  }
  // Overlapping labels are nudged apart so every message's label stays readable; each label stays on its own line's side.
  const placed: MapTalk[] = [];
  for (const talk of talks) {
    if (talk.label === "") continue;
    let y = talk.y;
    for (let step = 1; step <= 8; step += 1) {
      const hit = placed.some((other) => Math.abs(other.x - talk.x) < 150 && Math.abs(other.y - y) < 24);
      if (!hit) break;
      y = talk.y + (step % 2 === 1 ? 26 : -26) * Math.ceil(step / 2);
    }
    talk.y = y;
    placed.push(talk);
  }
  const width = workers.length === 0 ? LEAD_W : SUB_X + SUB_W;
  return { width, height: Math.max(contentH, CARD_H), cards, subs, starts, talks };
}

// The swim lanes of Crew Designs' lanes-main: one lane per member over a time axis taken from the messages, each message an
// arrow between its lanes, and each worker's brief outcome as a bar over its run.
const LANE_NAME_W = 140;
const LANE_ROW_H = 62;
const LANE_TOP = 39;
const LANE_PLOT_W = 550;
const TICK_STEPS = [600_000, 1_800_000, 3_600_000, 10_800_000, 21_600_000, 43_200_000, 86_400_000];

export interface SwimRow {
  node: FlowNode;
  /** The lane's top; its text starts here and its bar sits below. */
  y: number;
}
export interface SwimTick {
  at: number;
  label: string;
  x: number;
}
export interface SwimArrow {
  d: string;
  /** The arrowhead's own path. */
  head: string;
  from: string;
  to: string;
  at: string;
  /** "live · a fact", or just the kind when the read has no delivery row. */
  label: string;
  /** The message's fixed kind label on its own ("a fact"). */
  kind: string;
  delivery: string | null;
  x: number;
  y: number;
  title: string;
}
export interface SwimBar {
  node: FlowNode;
  x: number;
  y: number;
  w: number;
  text: string;
}
export interface SwimView {
  width: number;
  height: number;
  rows: SwimRow[];
  ticks: SwimTick[];
  arrows: SwimArrow[];
  bars: SwimBar[];
}

/** The lane's vertical middle, where an arrow leaves or arrives. */
const laneMid = (y: number): number => y + 15;

/**
 * The swim lanes' positions and arrows. The time axis starts at the 10-minute mark at or before the earliest message and ends at
 * one at or after the newest, widening its step rather than crowding the axis. A message to everyone is drawn to every other
 * lane and labelled once. The lane's level is not in any read, so nothing here has one.
 */
export function swimLayout(nodes: FlowNode[], messages: FlowMessage[], now: number = Date.now()): SwimView {
  const rows: SwimRow[] = nodes.map((node, at) => ({ node, y: LANE_TOP + at * LANE_ROW_H }));
  // Room for the last tick's label and its arrows; the plot keeps the design's own width.
  const width = LANE_NAME_W + LANE_PLOT_W + 24;
  const height = LANE_TOP + Math.max(0, rows.length - 1) * LANE_ROW_H + 64;
  const times = messages.map((message) => Date.parse(message.at)).filter((at) => !Number.isNaN(at));
  const first = times.length > 0 ? Math.min(...times) : null;
  const last = times.length > 0 ? Math.max(...times) : null;
  let step = TICK_STEPS[0]!;
  let start = 0;
  let end = 0;
  if (first !== null && last !== null) {
    start = Math.floor(first / step) * step;
    end = Math.ceil(last / step) * step;
    for (const candidate of TICK_STEPS) {
      step = candidate;
      start = Math.floor(first / step) * step;
      end = Math.ceil(last / step) * step;
      if ((end - start) / step <= 6) break;
    }
    if (end === start) end = start + step;
  }
  const ticks: SwimTick[] = [];
  if (first !== null && last !== null) {
    for (let at = start; at <= end; at += step) ticks.push({ at, label: clock(new Date(at).toISOString()), x: Math.min(width - 18, LANE_NAME_W + ((at - start) / (end - start)) * LANE_PLOT_W) });
  }
  const timeX = (at: number): number => (end === start ? LANE_NAME_W : LANE_NAME_W + ((at - start) / (end - start)) * LANE_PLOT_W);
  const byMember = new Map(rows.map((row) => [row.node.member, row]));
  const arrows: SwimArrow[] = [];
  if (first !== null) {
    for (const message of messages) {
      const at = Date.parse(message.at);
      const from = byMember.get(message.from);
      if (Number.isNaN(at) || !from) continue;
      const delivery = deliveryWord(message.delivery);
      const targets = message.to === "all" ? rows.filter((row) => row !== from) : [byMember.get(message.to)].filter((row): row is SwimRow => row !== undefined);
      const x1 = Math.min(width - 40, timeX(at) + 6);
      let labelled = false;
      for (const to of targets) {
        if (to.node === from.node) continue;
        const x2 = Math.min(width - 4, x1 + 32);
        const y1 = laneMid(from.y);
        const y2 = laneMid(to.y);
        const length = Math.hypot(x2 - x1, y2 - y1) || 1;
        const ux = (x2 - x1) / length;
        const uy = (y2 - y1) / length;
        const back = 6;
        const side = 3;
        arrows.push({
          d: "M" + x1 + "," + y1 + " L" + x2 + "," + y2,
          head: "M" + (x2 - back * ux + side * uy) + "," + (y2 - back * uy - side * ux) + " L" + x2 + "," + y2 + " L" + (x2 - back * ux - side * uy) + "," + (y2 - back * uy + side * ux) + " Z",
          from: message.from,
          to: message.to,
          at: message.at,
          label: labelled ? "" : [delivery, message.label].filter(Boolean).join(" · "),
          kind: labelled ? "" : message.label,
          delivery,
          x: Math.min(width - 80, Math.max(LANE_NAME_W + 60, (x1 + x2) / 2)),
          y: (y1 + y2) / 2,
          title: clock(message.at) + " " + message.from + " → " + message.to + ": " + message.label + (delivery ? ", " + delivery : ""),
        });
        labelled = true;
      }
    }
  }
  // Overlapping message labels are nudged apart the way the tree's are, so every message reads.
  const placed: SwimArrow[] = [];
  for (const arrow of arrows) {
    if (arrow.label === "") continue;
    let y = arrow.y;
    for (let step = 1; step <= 6; step += 1) {
      const hit = placed.some((other) => Math.abs(other.x - arrow.x) < 170 && Math.abs(other.y - y) < 24);
      if (!hit) break;
      y = arrow.y + (step % 2 === 1 ? 24 : -24) * Math.ceil(step / 2);
    }
    arrow.y = y;
    placed.push(arrow);
  }
  const bars: SwimBar[] = [];
  if (first !== null) {
    for (const row of rows) {
      if (row.node.role === "lead" || !row.node.outcome) continue;
      const from = row.node.created_at ? Date.parse(row.node.created_at) : Number.NaN;
      const to = row.node.updated_at ? Date.parse(row.node.updated_at) : now;
      const x = Number.isNaN(from) ? LANE_NAME_W : Math.max(LANE_NAME_W, timeX(from));
      const close = Number.isNaN(to) ? x : Math.min(width, timeX(to));
      bars.push({ node: row.node, x, y: row.y + 14, w: Math.max(0, close - x), text: row.node.outcome });
    }
  }
  return { width, height, rows, ticks, arrows, bars };
}
