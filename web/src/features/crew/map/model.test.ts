import { describe, expectTypeOf, expect, it } from "vitest";
import type { FlowFact } from "../../../generated/FlowFact";
import type { FlowMessage } from "../../../generated/FlowMessage";
import type { FlowNode } from "../../../generated/FlowNode";
import { checkWord, clock, deliveryWord, engineProblem, engineStatusWord, findingStatusWord, kindLabel, laneFacts, leaseWord, modelWord, ordered, quietWord, stateWord, subagentsWord } from "./model";

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
    expect(engineProblem("codex", "sign_in_unconfirmed")).toBe("Sign-in to codex isn’t confirmed.");
    expect(engineProblem("codex", "broken")).toBe("codex can’t start.");
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
