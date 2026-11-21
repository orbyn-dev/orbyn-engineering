import { describe, expect, it } from "vitest";
import { routeLine, type EngineTarget } from "./route";

const ready: EngineTarget = { name: "Claude", model: "Sonnet", availability: "ready" };
const off: EngineTarget = { ...ready, availability: "offline" };
const out: EngineTarget = { ...ready, availability: "signed-out" };

describe("routeLine", () => {
  it("sends a question to the engine in Suggest and says so", () => {
    const route = routeLine("suggest", "why is the build failing?", ready);
    expect(route).toMatchObject({ target: "engine", confirm: false });
    expect(route.note).toContain("prompt to Claude (Sonnet)");
  });
  it("keeps commands and ambiguous lines in the shell, and says nothing is sent", () => {
    for (const line of ["git status", "make sure the tests pass", "hello"]) {
      const route = routeLine("suggest", line, ready);
      expect(route.target).toBe("shell");
      expect(route.note).toContain("Nothing is sent");
    }
  });
  it("Esc keeps a suggested prompt in the shell", () => {
    expect(routeLine("suggest", "why is it slow?", ready, true).target).toBe("shell");
  });
  it("Shell mode never sends, even a sentence", () => {
    expect(routeLine("shell", "why is it slow?", ready).target).toBe("shell");
  });
  it("Engine mode sends a sentence and asks twice for something that looks like a command", () => {
    expect(routeLine("engine", "fix the flaky test", ready)).toMatchObject({ target: "engine", confirm: false });
    expect(routeLine("engine", "ls -la | grep x", ready)).toMatchObject({ target: "engine", confirm: true });
  });
  it("never runs a prompt-looking line in the shell when the engine is out of reach", () => {
    expect(routeLine("suggest", "why is it slow?", off)).toMatchObject({ target: "blocked", why: "offline", canRunInShell: true });
    expect(routeLine("suggest", "why is it slow?", out)).toMatchObject({ target: "blocked", why: "signed-out" });
    expect(routeLine("engine", "fix it please now", off).target).toBe("blocked");
  });
  it("shell commands still run while the engine is out of reach", () => {
    expect(routeLine("suggest", "git status", off).target).toBe("shell");
  });
  it("a secret never reaches the engine in any mode", () => {
    const secret = "why does export OPENAI_API_KEY=abc123 fail?";
    expect(routeLine("suggest", secret, ready).target).toBe("shell");
    expect(routeLine("shell", secret, ready).target).toBe("shell");
    expect(routeLine("engine", secret, ready)).toMatchObject({ target: "blocked", why: "secret" });
  });
  it("an empty line is the shell", () => expect(routeLine("engine", "  ", ready).target).toBe("shell"));
});
