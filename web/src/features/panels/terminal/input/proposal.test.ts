import { describe, expect, it } from "vitest";
import { proposedCommandIn } from "./proposal";

const tag = (body: string, note = "") => `Here you go.\n<proposed_command>${body}</proposed_command>${note}`;

describe("proposedCommandIn", () => {
  it("reads the command and the line that says what it does", () => {
    const found = proposedCommandIn(tag("git diff --name-only main...HEAD", "<proposed_note>Lists the files changed. It does not change files.</proposed_note>"));
    expect(found).toEqual({ command: "git diff --name-only main...HEAD", note: "Lists the files changed. It does not change files.", secret: false });
  });
  it("finds nothing when the engine did not propose a command", () => {
    expect(proposedCommandIn("Run `git status` to see.")).toBeNull();
    expect(proposedCommandIn(tag("   "))).toBeNull();
  });
  it("strips one wrapping code fence", () => {
    expect(proposedCommandIn(tag("```sh\nls -la\n```"))?.command).toBe("ls -la");
  });
  it("offers only one plain line: more lines, control characters and a huge command are not offered", () => {
    expect(proposedCommandIn(tag("ls\nrm -rf x"))).toBeNull();
    expect(proposedCommandIn(tag("ls \u001b[2J"))).toBeNull();
    expect(proposedCommandIn(tag("ls\u0003"))).toBeNull();
    expect(proposedCommandIn(tag(`echo ${"a".repeat(1100)}`))).toBeNull();
  });
  it("marks a secret-looking command so it is never filled in or run", () => {
    expect(proposedCommandIn(tag("export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx"))?.secret).toBe(true);
  });
  it("caps the note and drops markup it does not own", () => {
    const found = proposedCommandIn(tag("ls", `<proposed_note>${"x".repeat(900)}</proposed_note>`));
    expect(found?.note?.length).toBeLessThanOrEqual(280);
  });
});
