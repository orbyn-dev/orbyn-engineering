import { beforeEach, describe, expect, it } from "vitest";
import type { WireThreadEvent } from "../../../../api/types";
import { acceptTypedProposal, offerProposal, proposalFromEvents, useProposalSource } from "./proposal";

const event = (seq: number, runId: string | null, value: unknown): WireThreadEvent =>
  ({ seq, thread_id: "th", run_id: runId, ts: "", event: { type: "CUSTOM", name: "orbyn.proposal", value } }) as unknown as WireThreadEvent;

beforeEach(() => useProposalSource.getState().reset());

describe("typed terminal proposal", () => {
  it("defaults to the service source and keeps a typed field", () => {
    expect(useProposalSource.getState().source).toBe("service");
    offerProposal("th", "r1", { command: "ls" });
    expect(useProposalSource.getState().byRun["th:r1"]).toMatchObject({ command: "ls", secret: false });
  });
  it("can be turned off for a test: nothing is offered", () => {
    useProposalSource.getState().setSource("none");
    offerProposal("th", "r1", { command: "ls" });
    expect(useProposalSource.getState().byRun).toEqual({});
  });
  it("keeps a valid command and note when the service reports the field", () => {
    offerProposal("th", "r1", { command: "git diff --name-only", note: "Lists files." });
    expect(useProposalSource.getState().byRun["th:r1"]).toEqual({ command: "git diff --name-only", note: "Lists files.", secret: false });
  });
  it("rejects an empty command, a second line, NUL or other control characters, and an overlong command", () => {
    for (const command of ["   ", "ls\nrm x", "ls\u0000", "ls \u001b[2J", "ls\u0003", "ls\u007f", `echo ${"a".repeat(4100)}`]) {
      expect(acceptTypedProposal({ command })).toBeNull();
    }
  });
  it("allows a tab inside the command", () => {
    expect(acceptTypedProposal({ command: "echo\ta" })?.command).toBe("echo\ta");
  });
  it("caps the note at 1024 characters and drops control characters from it", () => {
    expect(acceptTypedProposal({ command: "ls", note: "x".repeat(2000) })?.note?.length).toBe(1024);
    expect(acceptTypedProposal({ command: "ls", note: "a\u001b[2Jb\nc" })?.note).toBe("a[2Jb c");
  });
  it("marks a secret-looking command", () => {
    expect(acceptTypedProposal({ command: "export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx" })?.secret).toBe(true);
  });
  it("does not read text: a tagged string is not a proposal", () => {
    expect(acceptTypedProposal("<proposed_command>ls</proposed_command>" as never)).toBeNull();
  });
});

describe("the orbyn.proposal event", () => {
  it("reads the last valid command and note for the run the editor started", () => {
    const events = [
      event(1, "r1", { command: "  git status  ", note: "Lists files." }),
      event(2, "r1", { command: 5 }),
      event(3, "r2", { command: "rm -rf x" }),
    ];
    expect(proposalFromEvents(events, "r1")).toEqual({ command: "git status", note: "Lists files.", secret: false });
    expect(proposalFromEvents(events, "r2")).toEqual({ command: "rm -rf x", note: null, secret: false });
    expect(proposalFromEvents(events, "r3")).toBeNull();
  });
  it("ignores another event name, a missing run id and a malformed value", () => {
    expect(proposalFromEvents([event(1, "r1", { command: "ls" })], "r1")).not.toBeNull();
    expect(proposalFromEvents([{ ...event(1, "r1", { command: "ls" }), event: { type: "CUSTOM", name: "other", value: { command: "ls" } } }], "r1")).toBeNull();
    expect(proposalFromEvents([event(1, null, { command: "ls" })], "r1")).toBeNull();
    for (const value of [null, "ls", 5, [], {}, { command: 5 }, { command: "ls", note: 5 }, { note: "no command" }]) {
      expect(proposalFromEvents([event(1, "r1", value)], "r1")).toBeNull();
    }
  });
  it("drops a multi-line, control-character or over-long command whole", () => {
    for (const command of ["ls\nrm x", "ls\rrm x", "ls\u0000", "ls \u001b[2J", `echo ${"a".repeat(4096)}`]) {
      expect(proposalFromEvents([event(1, "r1", { command })], "r1")).toBeNull();
    }
  });
  it("marks a secret-looking command instead of dropping it", () => {
    expect(proposalFromEvents([event(1, "r1", { command: "export TOKEN=sk-abcdefghijklmnop" })], "r1")?.secret).toBe(true);
  });
});
