import { describe, expectTypeOf, expect, it } from "vitest";
import type { FlowFact } from "../../../generated/FlowFact";
import type { FlowMessage } from "../../../generated/FlowMessage";
import type { FlowNode } from "../../../generated/FlowNode";
import type { FlowEdge } from "../../../generated/FlowEdge";
import { checkWord, clock, deliveryWord, elapsedWord, engineProblem, engineStatusWord, findingStatusWord, kindLabel, laneFacts, leaseWord, modelWord, ordered, quietWord, stateWord, subagentsWord, swimLayout, treeLayout } from "./model";

describe("the node's words", () => {
  it("words a task's state, with Needs you the only amber, Failed red and the lead plain", () => {
    expect(stateWord({ role: "lead", state: null })).toEqual({ text: "Lead", tone: "neutral" });
    expect(stateWord({ role: "worker", state: "running" })).toEqual({ text: "Running", tone: "info" });
    expect(stateWord({ role: "worker", state: "changed_approach" })).toEqual({ text: "Running", tone: "info" });
    expect(stateWord({ role: "worker", state: "needs_person" })).toEqual({ text: "Needs you", tone: "warn" });
    expect(stateWord({ role: "worker", state: "failed" })).toEqual({ text: "Failed", tone: "err" });
    expect(stateWord({ role: "worker", state: "passed" }).tone).toBe("ok");
    expect(stateWord({ role: "worker", state: "briefed" }).text).toBe("Queued");
    // There is no Blocked state on the wire, so none is drawn.
    expect(["briefed", "routed", "running", "changed_approach", "verifying", "needs_person", "failed", "passed", "stopped", "cancelled"].map((state) => stateWord({ role: "worker", state }).text)).not.toContain("Blocked");
  });

  it("says nothing for an active worker, quiet in quiet ink, and may be stuck as the one that asks", () => {
    expect(quietWord("active")).toBeNull();
    expect(quietWord("quiet")).toEqual({ text: "Quiet · 5 min", tone: "neutral" });
    expect(quietWord("may_be_stuck")).toEqual({ text: "May be stuck", tone: "warn" });
  });

  it("words the engine registry's five states, with no offline value", () => {
    expect((["ready", "not_installed", "signed_out", "sign_in_unconfirmed", "broken"] as const).map(engineStatusWord)).toEqual(["Ready", "Not installed", "Signed out", "Sign-in unconfirmed", "Broken"]);
    expect(engineProblem("codex", "ready")).toBeNull();
    // The offline frame is drawn from not_installed and the signed-out frame from signed_out.
    expect(engineProblem("codex", "not_installed")).toBe("codex isn’t running.");
    expect(engineProblem("codex", "signed_out")).toBe("codex isn’t signed in.");
    expect(engineProblem("codex", "sign_in_unconfirmed")).toBe("codex: Sign-in unconfirmed");
    expect(engineProblem("codex", "broken")).toBe("codex: Broken");
    // For the two without a frame, the banner carries the same word as the chip.
    for (const status of ["sign_in_unconfirmed", "broken"] as const) expect(engineProblem("x", status)).toContain(engineStatusWord(status));
  });

  it("says an engine does not report its subagents, and says nothing else about the ones that do", () => {
    expect(subagentsWord("not_reported")).toBe("Subagents not reported");
    expect(subagentsWord("reported")).toBeNull();
  });

  it("words the lease, the model and the order", () => {
    expect(leaseWord({ lease: ["a/*.ts"], read_only: false })).toBe("a/*.ts");
    expect(leaseWord({ lease: ["a"], read_only: true })).toBe("read only");
    expect(leaseWord({ lease: [], read_only: false })).toBe("no files");
    expect(modelWord({ model: "Sonnet 5.5", effort: "medium" })).toBe("Sonnet 5.5 · Medium");
    expect(modelWord({ model: null, effort: null })).toBe("");
    const nodes = [{ member: "w", role: "worker" }, { member: "l", role: "lead" }] as FlowNode[];
    expect(ordered(nodes).map((node) => node.member)).toEqual(["l", "w"]);
  });
});

describe("the message line", () => {
  it("words delivery, and shows nothing when the service gives none: it is never guessed", () => {
    expect((["live", "next_turn", "tool_result", "held", "failed"] as const).map(deliveryWord)).toEqual(["live", "next turn", "tool result", "held", "failed"]);
    expect(deliveryWord(null)).toBeNull();
    expect(deliveryWord(undefined)).toBeNull();
  });

  it("labels a kind with the service's label, else the fixed one, else a message", () => {
    expect(kindLabel("question", "a question")).toBe("a question");
    expect(kindLabel("blocker", "")).toBe("a blocker");
    expect(kindLabel("answer", null)).toBe("an answer");
    expect(kindLabel("whatever")).toBe("a message");
    expect(clock("not a time")).toBe("not a time");
    expect(clock("2026-11-07T14:09:00Z")).toMatch(/^\d\d:\d\d$/);
  });

  it("has no field that could hold a message's text or a fact's value (the types are labels only)", () => {
    expectTypeOf<FlowMessage>().not.toHaveProperty("body");
    expectTypeOf<FlowMessage>().not.toHaveProperty("text");
    expectTypeOf<FlowFact>().not.toHaveProperty("value");
    expectTypeOf<FlowNode>().not.toHaveProperty("thread");
    expectTypeOf<FlowNode>().not.toHaveProperty("tokens");
    expectTypeOf<FlowNode>().not.toHaveProperty("level");
  });
});

describe("findings and reviewer lanes", () => {
  it("uses the wire words: not checked yet for unchecked and pending, unconfirmed, and no Dropped", () => {
    expect(["confirmed", "unconfirmed", "pending", "unchecked"].map(checkWord)).toEqual(["Confirmed", "Unconfirmed", "Not checked yet", "Not checked yet"]);
    expect(["open", "fixed", "regressed", "superseded", "unreported"].map(findingStatusWord)).toEqual(["Open", "Fixed", "Regressed", "Superseded", "Not reported since"]);
    expect([...["confirmed", "unconfirmed", "pending", "unchecked"].map(checkWord), ...["open", "fixed", "regressed", "superseded", "unreported"].map(findingStatusWord)]).not.toContain("Dropped");
  });

  it("says how a lane ran: its tier, the commit it read and whether it was the author's engine; nothing for what is unknown", () => {
    expect(laneFacts({ tier: "lead", commit: "a41c9e2c0ffee", same_engine: false })).toEqual(["lead tier", "read a41c9e2", "Different engine from the author"]);
    expect(laneFacts({ tier: null, commit: null, same_engine: true })).toEqual(["Same engine as author"]);
    expect(laneFacts({})).toEqual([]);
  });
});

const node = (member: string, role: "lead" | "worker", extra: Partial<FlowNode> = {}): FlowNode => ({
  member,
  role,
  engine: "claude",
  model: "Sonnet 5.5",
  effort: "medium",
  state: role === "lead" ? null : "running",
  quiet: "active",
  engine_status: "ready",
  subagents: "reported",
  outcome: "Use next when it points inside the app",
  lease: ["auth/redirect.ts"],
  read_only: false,
  unread: 0,
  started_by: role === "lead" ? null : "lead",
  created_at: "2026-11-07T14:30:00Z",
  updated_at: "2026-11-07T14:58:00Z",
  ...extra,
});

describe("the wide tree's layout", () => {
  const nodes = [node("lead", "lead", { unread: 0, lease: [], outcome: "" }), node("implement", "worker", { unread: 1 }), node("tests", "worker", { engine: "codex" })];
  const edges: FlowEdge[] = [
    { from: "implement", to: "tests", kind: "fact", label: "a fact", count: 1, delivery: "live" },
    { from: "lead", to: "all", kind: "answer", label: "an answer", count: 2, delivery: null },
  ];

  it("puts the lead in the first column, one worker under another, and each worker's subagent report beside it", () => {
    const view = treeLayout(nodes, edges);
    expect(view.cards.map((card) => [card.node.member, card.x, card.y])).toEqual([
      ["lead", 0, 80],
      ["implement", 290, 0],
      ["tests", 290, 160],
    ]);
    expect(view.subs.map((sub) => [sub.node.member, sub.x, sub.y])).toEqual([
      ["implement", 596, 0],
      ["tests", 596, 160],
    ]);
    expect(view.width).toBe(832);
    expect(view.height).toBe(290);
  });

  it("draws a solid line from the lead to each worker it started and a dashed line for each message, with the message's own label at the middle", () => {
    const view = treeLayout(nodes, edges);
    expect(view.starts.map((path) => path.d)).toEqual(["M232,145 H261 V65 H290", "M232,145 H261 V225 H290"]);
    const talks = view.talks.filter((edge) => edge.to !== "all");
    expect(talks).toHaveLength(1);
    expect(talks[0]!.d).toBe("M300,130 V160");
    expect(talks[0]!.label).toBe("a fact");
    expect(talks[0]!.delivery).toBe("live");
    expect(talks[0]!.title).toBe("implement → tests: 1 message, a fact, newest live");
    // A message to everyone is drawn to each member, and its label says so once.
    const broadcast = view.talks.filter((edge) => edge.to === "all");
    expect(broadcast.map((edge) => edge.from + "→" + edge.to).sort()).toEqual(["lead→all", "lead→all"]);
    expect(broadcast.filter((edge) => edge.label !== "").length).toBe(1);
  });

  it("draws no edge for a member the map does not carry, and none from a node to itself", () => {
    const view = treeLayout([nodes[0]!, nodes[1]!], [
      { from: "implement", to: "ghost", kind: "fact", label: "a fact", count: 1, delivery: null },
      { from: "lead", to: "lead", kind: "fact", label: "a fact", count: 1, delivery: null },
    ]);
    expect(view.talks).toHaveLength(0);
    expect(view.starts).toHaveLength(1);
  });
});

describe("the swim lanes' layout", () => {
  const at = (minute: number) => new Date(Date.parse("2026-11-07T15:00:00Z") - minute * 60_000).toISOString();
  const nodes = [node("lead", "lead", { created_at: at(60), updated_at: at(1) }), node("implement", "worker", { created_at: at(36), updated_at: at(2) }), node("tests", "worker", { engine: "codex", created_at: at(30), updated_at: at(3) })];
  const messages: FlowMessage[] = [
    { id: "m1", at: at(36), from: "implement", to: "tests", kind: "fact", label: "a fact", delivery: "live" },
    { id: "m2", at: at(20), from: "lead", to: "all", kind: "answer", label: "an answer", delivery: null },
    { id: "m3", at: at(2), from: "tests", to: "lead", kind: "handoff", label: "a handoff", delivery: "tool_result" },
  ];
  const now = Date.parse("2026-11-07T15:00:00Z");

  it("puts one lane per member, the lead first, on a time axis taken from the messages", () => {
    const view = swimLayout(nodes, messages, now);
    expect(view.rows.map((row) => [row.node.member, row.y])).toEqual([["lead", 39], ["implement", 101], ["tests", 163]]);
    // The axis starts at the 10-minute mark at or before the earliest message and ends at the one at or after the newest.
    const labels = view.ticks.map((tick) => tick.label);
    expect(labels.length).toBeGreaterThanOrEqual(3);
    expect(view.ticks[0]!.at % 600_000).toBe(0);
    expect(view.ticks.at(-1)!.at % 600_000).toBe(0);
    expect(view.ticks.at(-1)!.at).toBeGreaterThanOrEqual(Date.parse(at(2)));
    expect(new Set(labels).size).toBe(labels.length);
    for (const tick of view.ticks) expect(tick.label).toMatch(/^\d\d:\d\d$/);
  });

  it("draws each message as an arrow from its sender's lane to its receiver's, labelled with how it arrived and its kind", () => {
    const view = swimLayout(nodes, messages, now);
    const direct = view.arrows.filter((arrow) => arrow.to !== "all");
    expect(direct).toHaveLength(2);
    expect(direct[0]!.label).toBe("live · a fact");
    expect(direct[0]!.title).toMatch(/^\d\d:\d\d implement → tests: a fact, live$/);
    expect(direct[1]!.label).toBe("tool result · a handoff");
    // A message to everyone goes to every other lane, with one label reading as the design's.
    const broadcast = view.arrows.filter((arrow) => arrow.to === "all");
    expect(broadcast.map((arrow) => arrow.from).sort()).toEqual(["lead", "lead"]);
    expect(broadcast.filter((arrow) => arrow.label !== "").length).toBe(1);
    for (const arrow of view.arrows) expect(arrow.d).toMatch(/^M[\d.]+,[\d.]+ L[\d.]+,[\d.]+$/);
  });

  it("draws a bar of each worker's outcome over its run, and none for a crew label or a lane level the map does not carry", () => {
    const view = swimLayout(nodes, messages, now);
    expect(view.bars.map((bar) => [bar.node.member, bar.text])).toEqual([
      ["implement", "Use next when it points inside the app"],
      ["tests", "Use next when it points inside the app"],
    ]);
    expect(view.bars.every((bar) => bar.w > 0)).toBe(true);
    expectTypeOf(view).not.toHaveProperty("levels");
  });

  it("draws no lane, tick or arrow before anything is said", () => {
    const view = swimLayout(nodes, [], now);
    expect(view.rows).toHaveLength(3);
    expect(view.ticks).toEqual([]);
    expect(view.arrows).toEqual([]);
    expect(view.bars.every((bar) => bar.w === 0)).toBe(true);
  });
});


describe("how long a worker has been going", () => {
  const start = "2026-11-07T14:00:00Z";
  const later = Date.parse("2026-11-10T14:00:00Z");
  it("runs a live worker's time to now", () => {
    expect(elapsedWord({ state: "running", created_at: start, updated_at: "2026-11-07T14:05:00Z" }, later)).toBe("3d");
  });
  it("ends a finished worker's time at its last update, however long ago", () => {
    for (const state of ["passed", "failed", "stopped", "cancelled"]) {
      expect(elapsedWord({ state, created_at: start, updated_at: "2026-11-07T14:28:30Z" }, later)).toBe("28m 30s");
    }
  });
  it("gives a finished worker with no readable last update no time, never a growing one", () => {
    expect(elapsedWord({ state: "passed", created_at: start, updated_at: null }, later)).toBeNull();
    expect(elapsedWord({ state: "failed", created_at: start, updated_at: "garbage" }, later)).toBeNull();
  });
  it("gives the lead, which has no task state, a running clock", () => {
    expect(elapsedWord({ state: null, created_at: start, updated_at: null }, Date.parse("2026-11-07T14:10:00Z"))).toBe("10m");
  });
});
