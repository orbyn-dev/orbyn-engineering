import type { Row } from "./diffModel";

/** One hunk of a patch as drawn: its header row and the rows under it, up to the next header. */
export interface Hunk {
  /** Row position of the header. */
  at: number;
  header: string;
  rows: Row[];
  newStart: number;
  newCount: number;
}

const HEADER = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/;

/** The hunks of the drawn rows, each with the numbers its header gives. */
export function hunksOf(rows: readonly Row[]): Hunk[] {
  const out: Hunk[] = [];
  for (const row of rows) {
    if (row.line.kind === "hunk") {
      const match = HEADER.exec(row.line.text);
      out.push({
        at: row.at,
        header: row.line.text,
        rows: [],
        newStart: Number(match?.[1] ?? 0),
        newCount: match?.[2] === undefined ? 1 : Number(match[2]),
      });
    } else out.at(-1)?.rows.push(row);
  }
  return out;
}

const bare = (text: string) => text.slice(1).replace(/\r$/, "");

/** What the hunk's lines say on each side: the file as it is now, and as the last commit has it. */
const sides = (hunk: Hunk) => ({
  now: hunk.rows.filter((row) => row.line.kind === "ctx" || row.line.kind === "add").map((row) => bare(row.line.text)),
  // A line put back keeps the ending git showed it with: a carriage return in the diff line is part of that line.
  before: hunk.rows.filter((row) => row.line.kind === "ctx" || row.line.kind === "del")
    .map((row) => `${bare(row.line.text)}${row.line.text.endsWith("\r") ? "\r\n" : "\n"}`),
});

/** A hunk can be put back when it changes something and every line of it is a plain line: a "no newline" mark is not. */
export const canRevert = (hunk: Hunk) =>
  hunk.rows.every((row) => row.line.kind !== "meta") && hunk.rows.some((row) => row.line.kind === "add" || row.line.kind === "del");

/** The lines a hunk changes, as the dialog says them: new-side numbers, or the old side for a pure removal. */
export function changedLines(hunk: Hunk): string {
  const added = hunk.rows.filter((row) => row.line.kind === "add" && row.newNo !== null).map((row) => row.newNo!);
  const numbers = added.length > 0 ? added : hunk.rows.filter((row) => row.line.kind === "del" && row.oldNo !== null).map((row) => row.oldNo!);
  const first = numbers[0] ?? hunk.newStart;
  const last = numbers.at(-1) ?? first;
  return first === last ? `Line ${first}` : `Lines ${first} to ${last}`;
}

/** The hunk as git writes it, for Copy hunk. */
export const hunkPatch = (hunk: Hunk) => [hunk.header, ...hunk.rows.map((row) => row.line.text)].join("\n");

/** The file's lines, each with the line ending it has. */
const linesOfText = (content: string) => content.match(/[^\n]*\n|[^\n]+$/g) ?? [];
const endingOf = (line: string) => /\r?\n$/.exec(line)?.[0] ?? "";
const textOf = (line: string) => line.replace(/\r?\n$/, "");

/**
 * The file with this hunk put back to the last commit, or null when the file no longer holds the lines
 * the diff showed there. Nothing is guessed: a file that moved on is left alone.
 */
export function revertedContent(content: string, hunk: Hunk): string | null {
  if (!canRevert(hunk)) return null;
  const { now, before } = sides(hunk);
  const lines = linesOfText(content);
  // A file emptied by the change has "+0,0": it must be empty now, or this is not the file the diff was read from.
  if (hunk.newStart === 0 && hunk.newCount === 0 && lines.length > 0) return null;
  const index = hunk.newCount === 0 ? hunk.newStart : hunk.newStart - 1;
  if (index < 0 || index + now.length > lines.length) return null;
  if (!now.every((text, at) => textOf(lines[index + at]!) === text)) return null;
  const put = [...before];
  // The last line of a file may have no ending of its own; the lines put back keep that.
  const last = lines[index + now.length - 1];
  if (last !== undefined && endingOf(last) === "" && put.length > 0) put[put.length - 1] = textOf(put.at(-1)!);
  return [...lines.slice(0, index), ...put, ...lines.slice(index + now.length)].join("");
}

/** The file with line `number` (one-based, new side) changed from `was` to `next`, or null when it no longer reads `was`. */
export function editedContent(content: string, number: number, was: string, next: string): string | null {
  const lines = linesOfText(content);
  const line = lines[number - 1];
  if (line === undefined || textOf(line) !== was.replace(/\r$/, "")) return null;
  lines[number - 1] = `${next}${endingOf(line)}`;
  return lines.join("");
}
