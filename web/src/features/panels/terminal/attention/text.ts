import { stripTerminalText } from "../blocks/blockText";
import { LIMITS, type AttentionEntry } from "./contract";

const ENGINES = new Map([["claude", "Claude Code"], ["codex", "Codex"]]);

/** Plain text of a bounded size: escapes and controls gone, one line, cut at `max`. */
export function oneLine(text: string, max: number): string {
  return stripTerminalText(text).replace(/\s+/g, " ").trim().slice(0, max);
}

/** The engine's name for a family id; an unknown id is drawn as text, capped. */
export function engineName(family: string): string {
  return ENGINES.get(family) ?? oneLine(family, LIMITS.engineChars);
}

/** The excerpt as lines: cleaned, each cut, at most the last few, at most the total. A secret keeps only its label. */
export function cleanExcerpt(lines: readonly string[], secret: boolean): string[] {
  if (secret) {
    const label = lines.map((line) => oneLine(line, LIMITS.labelChars)).find((line) => line !== "");
    return label ? [label] : [];
  }
  const kept = lines
    .flatMap((line) => stripTerminalText(line).split("\n"))
    .map((line) => line.replace(/\s+$/, "").slice(0, LIMITS.lineChars))
    .filter((line) => line !== "")
    .slice(-LIMITS.lines);
  let total = 0;
  const out: string[] = [];
  for (const line of [...kept].reverse()) {
    if (total + line.length > LIMITS.excerptChars) break;
    total += line.length + 1;
    out.unshift(line);
  }
  return out;
}

export const itemTitle = (entry: AttentionEntry): string =>
  entry.kind === "secret"
    ? `${engineName(entry.engine)} asked for a password in the terminal “${entry.terminalTitle}”.`
    : `${engineName(entry.engine)} in the terminal “${entry.terminalTitle}” is waiting for an answer.`;

export function whyLine(entry: AttentionEntry, now: number): string {
  if (entry.kind === "secret") return "Orbyn does not take passwords, tokens or keys here. Type it in the terminal, where it stays hidden.";
  const seconds = Math.max(0, Math.round((now - entry.since) / 1000));
  return `It stopped printing ${seconds} s ago, and its last line reads like a question.`;
}

/** What a secret prompt says outside the terminal: the engine and nothing else, never the prompt, its label or the terminal's title. */
const secretText = (entry: AttentionEntry): string => `${engineName(entry.engine)} is waiting for a password`;

/** The toast: names the engine and where, never the prompt and never an answer. A secret names only the engine. */
export const toastText = (entry: AttentionEntry, threadTitle: string): string =>
  entry.kind === "secret"
    ? secretText(entry)
    : `${engineName(entry.engine)} needs you in “${entry.terminalTitle}”${threadTitle === "" ? "" : ` · ${oneLine(threadTitle, 80)}`}`;

/** The system notification's body. For a secret it never carries the prompt. */
export function notificationText(entry: AttentionEntry, threadTitle: string): string {
  if (entry.kind === "secret") return secretText(entry);
  const question = entry.excerpt.at(-1) ?? "";
  const where = `${engineName(entry.engine)} in “${entry.terminalTitle}”${threadTitle === "" ? "" : ` · ${oneLine(threadTitle, 80)}`}`;
  return question === "" ? `${where} is waiting for an answer` : `${where}: ${question}`;
}

export const clearedLine = (entry: AttentionEntry): string => {
  const who = `${engineName(entry.engine)} in the terminal “${entry.terminalTitle}”`;
  const reason = entry.cleared?.reason;
  const when = entry.cleared ? ` · ${new Date(entry.cleared.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : "";
  if (reason === "input") return `Answered in the terminal “${entry.terminalTitle}”${when}`;
  if (reason === "closed") return `The terminal “${entry.terminalTitle}” closed${when}`;
  return `${who} continued${when}`;
};
