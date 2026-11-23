/* eslint-disable no-control-regex -- escape and control characters are exactly what these patterns match. */
/**
 * Text from a terminal is untrusted: a program can print markup, escape sequences or a fake prompt. These helpers make it plain text
 * of a bounded size before the blocks draw it or copy it. Nothing here parses markup, and nothing that comes out is ever drawn as HTML.
 */

/** The longest command line a block keeps (docs/command-blocks.md). */
export const COMMAND_MAX = 4096;
/** The most characters of one block's output the web keeps in memory; the tail is kept. */
export const OUTPUT_MAX_CHARS = 400_000;

// OSC (to BEL or ST), DCS/PM/APC/SOS (to ST), CSI, then any other two-character escape.
const OSC = /\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)?/g;
const STRING_SEQUENCE = /\u001b[PX^_][^\u001b]*(?:\u001b\\)?/g;
const CSI = /\u001b\[[0-?]*[ -/]*[@-~]/g;
const OTHER_ESCAPE = /\u001b[ -/]*[0-~]?/g;
// C0 except tab and newline, DEL and the C1 range.
const CONTROL = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g;

export function stripTerminalText(text: string): string {
  const plain = text.replace(OSC, "").replace(STRING_SEQUENCE, "").replace(CSI, "").replace(OTHER_ESCAPE, "");
  return plain
    .replace(/\r\n/g, "\n")
    .split("\n")
    // A carriage return redraws the line: what was written last is what the person saw.
    .map((line) => (line.includes("\r") ? (line.split("\r").filter((part) => part !== "").at(-1) ?? "") : line))
    .join("\n")
    .replace(CONTROL, "");
}

/** Keeps the end of `text` when it is longer than `max`. */
export function capTail(text: string, max: number): { text: string; cut: boolean } {
  return text.length > max ? { text: text.slice(text.length - max), cut: true } : { text, cut: false };
}

/** A command line as a block header draws it: one plain line of at most 4096 characters, or null when there is nothing to show. */
export function cleanCommand(command: string | null): string | null {
  if (command === null) return null;
  const line = stripTerminalText(command).replace(/\s+/g, " ").trim();
  return line === "" ? null : line.slice(0, COMMAND_MAX);
}

/**
 * The text Run again writes into the terminal: the command exactly as the service reported it. A command that is empty, too long, or
 * holds a newline or another control character is refused whole, never repaired, so a click can only ever put one visible line in the
 * terminal's input.
 */
export function rerunText(command: string | null): string | null {
  if (command === null || command.trim() === "" || command.length > COMMAND_MAX) return null;
  if (/[\u0000-\u0008\u000a-\u001f\u007f-\u009f]/.test(command)) return null;
  return command;
}

/** How long a finished command took: tenths under ten seconds, then whole seconds, minutes and hours. */
export function durationWords(startedIso: string, endedIso: string): string {
  const ms = new Date(endedIso).getTime() - new Date(startedIso).getTime();
  if (Number.isNaN(ms)) return "";
  const clamped = Math.max(0, ms);
  if (clamped === 0 && ms < 0) return "0 s";
  if (clamped < 10_000) return `${(Math.round(clamped / 100) / 10).toFixed(1)} s`;
  const seconds = Math.round(clamped / 1000);
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} m ${String(seconds % 60).padStart(2, "0")} s`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, "0")} m`;
}

/** The live clock of a running command: `0:42`, or `1:02:03` past an hour. */
export function runningClock(startedIso: string, nowMs: number): string {
  const from = new Date(startedIso).getTime();
  if (Number.isNaN(from)) return "";
  const total = Math.max(0, Math.floor((nowMs - from) / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}` : `${minutes}:${seconds}`;
}

/** The lines of `text`; one final newline does not start a line. */
export function outputLines(text: string): string[] {
  if (text === "") return [];
  return (text.endsWith("\n") ? text.slice(0, -1) : text).split("\n");
}

/** The last `count` lines of `text` and how many it has in all; one final newline does not start a line. */
export function lastLines(text: string, count: number): { text: string; total: number; cut: boolean } {
  const lines = outputLines(text);
  return lines.length > count
    ? { text: lines.slice(lines.length - count).join("\n"), total: lines.length, cut: true }
    : { text: lines.join("\n"), total: lines.length, cut: false };
}

export interface TextMatch {
  start: number;
  end: number;
}

/**
 * Every non-overlapping match of `query` in `text`, by offset, ignoring case. It is plain text, never a pattern: a dot matches a dot.
 * An empty query has no matches. Adjacent matches are found; a match that would overlap an earlier one is not counted twice.
 */
export function findTextMatches(text: string, query: string): TextMatch[] {
  if (query === "") return [];
  const hay = text.toLowerCase();
  const needle = query.toLowerCase();
  const found: TextMatch[] = [];
  for (let at = hay.indexOf(needle); at >= 0; at = hay.indexOf(needle, at + needle.length)) found.push({ start: at, end: at + needle.length });
  return found;
}

/**
 * The lines of `text` a filter keeps: those holding `query` as plain text, ignoring case. An empty query keeps every line. `total` counts
 * the block's held lines and `kept` how many remain. One final newline does not start a line.
 */
export function filterBlockLines(text: string, query: string): { text: string; total: number; kept: number } {
  const lines = outputLines(text);
  if (query === "") return { text: lines.join("\n"), total: lines.length, kept: lines.length };
  const needle = query.toLowerCase();
  const kept = lines.filter((line) => line.toLowerCase().includes(needle));
  return { text: kept.join("\n"), total: lines.length, kept: kept.length };
}
