import { describe, expect, it } from "vitest";
import { engineTools } from "../../../mock/fixtures";
import { notReportedNotes, problemLine, summarize } from "./toolsModel";

describe("the tools summary", () => {
  it("counts Orbyn tools, MCP servers, connectors and plugins, and lists built-ins and skills uncounted", () => {
    const summary = summarize(engineTools("claude"));
    // 5 Orbyn tools + github + Vercel + orbyn-skills ready; the 3 built-ins and 39 skills are not counted.
    expect(summary).toEqual({ ready: 8, starting: 0, needsSignIn: 1, failed: 1 });
  });

  it("names only what is not ready, failures and sign-ins before starting", () => {
    expect(problemLine({ ready: 8, starting: 1, needsSignIn: 1, failed: 1 })).toBe("1 needs sign-in · 1 failed · 1 starting");
    expect(problemLine({ ready: 8, starting: 0, needsSignIn: 0, failed: 0 })).toBeNull();
  });

  it("reads Codex's list: a starting app is neither ready nor a problem to fix", () => {
    const summary = summarize(engineTools("codex"));
    expect(summary).toEqual({ ready: 8, starting: 1, needsSignIn: 1, failed: 0 });
  });

  it("says what an engine does not report, in its own name", () => {
    expect(notReportedNotes("Codex", engineTools("codex"))).toEqual(["Codex doesn’t report its built-in tools."]);
    expect(notReportedNotes("Claude Code", engineTools("claude"))).toEqual([]);
  });
});
