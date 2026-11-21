import { classifyLine, looksSecret } from "./classify";

export type Mode = "suggest" | "shell" | "engine";

export interface EngineTarget {
  name: string;
  model: string | null;
  availability: "ready" | "offline" | "signed-out";
}

/** What Enter does with the line, and the one sentence that tells the person so. The note is always shown. */
export type Route =
  | { target: "shell"; note: string }
  | { target: "engine"; note: string; /** A line that looks like a command needs a second Enter. */ confirm: boolean }
  | { target: "blocked"; note: string; why: "secret" | "offline" | "signed-out"; canRunInShell: boolean };

const where = (engine: EngineTarget) => (engine.model ? `${engine.name} (${engine.model})` : engine.name);

const unavailable = (engine: EngineTarget): Route => ({
  target: "blocked",
  why: engine.availability === "offline" ? "offline" : "signed-out",
  canRunInShell: true,
  note:
    engine.availability === "offline"
      ? `${engine.name} can't be reached, so this was not sent. It is not run in the shell either.`
      : `${engine.name} is not signed in, so this was not sent. It is not run in the shell either.`,
});

/**
 * Where a line goes. Shell is the default for anything unclear, a secret never goes to the engine, and a line the
 * classifier reads as a prompt is never run in the shell without the person choosing that.
 * `keepInShell` is the person pressing Esc on a suggestion.
 */
export function routeLine(mode: Mode, line: string, engine: EngineTarget, keepInShell = false): Route {
  if (line.trim() === "") return { target: "shell", note: "Runs in the shell as typed. Nothing is sent to the engine." };
  if (looksSecret(line)) {
    return mode === "engine"
      ? { target: "blocked", why: "secret", canRunInShell: true, note: `Looks like a secret, so it is not sent to ${engine.name}. Switch to Shell to run it.` }
      : { target: "shell", note: `Looks like a secret. It runs in the shell only and is never sent to ${engine.name}.` };
  }
  const effective: Mode = keepInShell ? "shell" : mode;
  if (effective === "shell") return { target: "shell", note: `Runs in the shell as typed. Nothing is sent to ${engine.name}.` };
  const { verdict, reason } = classifyLine(line);
  if (effective === "engine") {
    if (engine.availability !== "ready") return unavailable(engine);
    return verdict === "shell"
      ? { target: "engine", confirm: true, note: `Looks like a command (${reason.toLowerCase()}). Press Enter again to send it to ${where(engine)} as a prompt, or switch to Shell.` }
      : { target: "engine", confirm: false, note: `Sending as a prompt to ${where(engine)}. No command runs in the shell.` };
  }
  if (verdict === "prompt") {
    if (engine.availability !== "ready") return unavailable(engine);
    return { target: "engine", confirm: false, note: `${reason}. Enter sends it as a prompt to ${where(engine)}; Esc keeps it in the shell.` };
  }
  return {
    target: "shell",
    note:
      verdict === "ambiguous"
        ? `${reason}, so it runs in the shell. Nothing is sent to ${engine.name}. Switch to Engine to send it.`
        : `Runs in the shell as typed. Nothing is sent to ${engine.name}.`,
  };
}
