import { describe, expect, it } from "vitest";
import { engineName, itemTitle, notificationText, toastText, whyLine } from "./text";

const entry = (over: Record<string, unknown> = {}) => ({ id: "a", threadId: "t1", terminalId: "tm", terminalTitle: "dev", engine: "claude", kind: "yes_no" as const, since: 0, toldAt: 0, excerpt: ["Overwrite? (y/N)"], seq: 1, dismissed: false, cleared: null, ...over });

describe("copy", () => {
  it("names the engine families and never an inherited key", () => {
    expect(engineName("claude")).toBe("Claude Code");
    expect(engineName("codex")).toBe("Codex");
    expect(engineName("toString")).toBe("toString");
    expect(engineName("x".repeat(80)).length).toBe(40);
  });
  it("titles an item as the frames do", () => {
    expect(itemTitle(entry())).toBe("Claude Code in the terminal “dev” is waiting for an answer.");
    expect(itemTitle(entry({ kind: "secret" }))).toBe("Claude Code asked for a password in the terminal “dev”.");
  });
  it("a notification for a question carries the question, and for a secret never the prompt", () => {
    expect(notificationText(entry(), "Fix login")).toContain("Overwrite? (y/N)");
    const secret = notificationText(entry({ kind: "secret", excerpt: ["Password:"] }), "Fix login");
    expect(secret).toBe("Claude Code is waiting for a password in “dev”");
    expect(secret).not.toContain("Password:");
    expect(toastText(entry({ kind: "secret", excerpt: ["Password:"] }), "Fix login")).not.toContain("Password:");
    expect(toastText(entry(), "Fix login")).toBe("Claude Code needs you in “dev” · Fix login");
  });
  it("says why, in seconds, and never invents more", () => {
    expect(whyLine(entry({ since: 1000 }), 39_000)).toBe("It stopped printing 38 s ago, and its last line reads like a question.");
    expect(whyLine(entry({ kind: "secret" }), 0)).toContain("Type it in the terminal");
  });
});
