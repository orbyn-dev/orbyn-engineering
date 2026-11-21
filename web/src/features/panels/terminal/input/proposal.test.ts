import { beforeEach, describe, expect, it } from "vitest";
import { acceptTypedProposal, offerProposal, useProposalSource } from "./proposal";

beforeEach(() => useProposalSource.getState().reset());

describe("typed terminal proposal", () => {
  it("is reported by nobody by default: production has no source, so nothing is offered", () => {
    expect(useProposalSource.getState().source).toBe("none");
    offerProposal("th", "r1", { command: "ls" });
    expect(useProposalSource.getState().byRun).toEqual({});
  });
  it("keeps a valid command and note when the service reports the field", () => {
    useProposalSource.getState().setSource("service");
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
