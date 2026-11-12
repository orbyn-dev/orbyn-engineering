/** What the editor says about a file, read from its bytes and its name. Pure: nothing here touches the page. */

const LANGUAGES: Record<string, { label: string; grammar: string | null }> = {
  ts: { label: "TypeScript", grammar: "typescript" },
  mts: { label: "TypeScript", grammar: "typescript" },
  cts: { label: "TypeScript", grammar: "typescript" },
  tsx: { label: "TypeScript React", grammar: "tsx" },
  js: { label: "JavaScript", grammar: "javascript" },
  mjs: { label: "JavaScript", grammar: "javascript" },
  cjs: { label: "JavaScript", grammar: "javascript" },
  jsx: { label: "JavaScript React", grammar: "jsx" },
  json: { label: "JSON", grammar: "json" },
  css: { label: "CSS", grammar: "css" },
  html: { label: "HTML", grammar: "html" },
  htm: { label: "HTML", grammar: "html" },
  md: { label: "Markdown", grammar: "markdown" },
  py: { label: "Python", grammar: "python" },
  rs: { label: "Rust", grammar: "rust" },
  go: { label: "Go", grammar: "go" },
  sql: { label: "SQL", grammar: "sql" },
  toml: { label: "TOML", grammar: "toml" },
  yml: { label: "YAML", grammar: "yaml" },
  yaml: { label: "YAML", grammar: "yaml" },
  sh: { label: "Shell", grammar: "bash" },
  zsh: { label: "Shell", grammar: "bash" },
  bash: { label: "Shell", grammar: "bash" },
  csv: { label: "CSV", grammar: null },
  txt: { label: "Plain text", grammar: null },
};

/** The language the status bar names and the grammar that colours it (null: no colour). */
export function languageOf(path: string): { label: string; grammar: string | null } {
  const name = path.split(/[\\/]/).at(-1) ?? path;
  const dot = name.lastIndexOf(".");
  const known = dot > 0 ? LANGUAGES[name.slice(dot + 1).toLowerCase()] : undefined;
  return known ?? { label: "Plain text", grammar: null };
}

export type Eol = "LF" | "CRLF" | "CR";

/** The file's line ending (the commonest of LF, CRLF and CR; LF on a tie) and whether it uses more than one. */
export function endingsOf(text: string): { eol: Eol; mixed: boolean } {
  let crlf = 0;
  let lf = 0;
  let cr = 0;
  for (const match of text.matchAll(/\r\n|\n|\r/g)) {
    if (match[0] === "\r\n") crlf += 1;
    else if (match[0] === "\n") lf += 1;
    else cr += 1;
  }
  const kinds = [crlf, lf, cr].filter((count) => count > 0).length;
  const eol: Eol = crlf > lf && crlf >= cr ? "CRLF" : cr > lf && cr > crlf ? "CR" : "LF";
  return { eol, mixed: kinds > 1 };
}

/** What the status bar says about the line ending. */
export const endingLabel = (eol: Eol, mixed: boolean): string => (mixed ? `${eol} (mixed)` : eol);

/** The editor holds every line break as a line feed, as a text field does; the file's own ending is put back on save. */
export const toLF = (text: string): string => text.replace(/\r\n?/g, "\n");
export const withEol = (text: string, eol: Eol): string => (eol === "CRLF" ? text.replace(/\n/g, "\r\n") : eol === "CR" ? text.replace(/\n/g, "\r") : text);

/** Tabs, or the commonest leading run of spaces (2 or 4) among the first lines. */
export function indentOf(text: string): string {
  const counts = new Map<number, number>();
  let tabs = 0;
  for (const line of text.split("\n", 200)) {
    if (line.startsWith("\t")) tabs += 1;
    else {
      const run = /^ +/.exec(line)?.[0].length ?? 0;
      if (run === 2 || run === 4 || run === 8) counts.set(run, (counts.get(run) ?? 0) + 1);
    }
  }
  const spaces = [...counts.entries()].sort((a, b) => b[1] - a[1])[0];
  if (tabs > (spaces?.[1] ?? 0)) return "Tabs";
  return spaces && spaces[0] !== 2 ? "4 spaces" : "2 spaces";
}

/** One-based line and column of a character offset. */
export function positionOf(text: string, offset: number): { line: number; col: number } {
  const before = text.slice(0, Math.max(0, Math.min(offset, text.length)));
  const line = before.split("\n").length;
  return { line, col: offset - (before.lastIndexOf("\n") + 1) + 1 };
}

/** The offset where a one-based line starts; a line past the end starts at the end. */
export function lineStart(text: string, line: number): number {
  let at = 0;
  for (let seen = 1; seen < line; seen += 1) {
    const next = text.indexOf("\n", at);
    if (next < 0) return text.length;
    at = next + 1;
  }
  return at;
}

export const lineCount = (text: string): number => {
  let count = 1;
  for (let at = text.indexOf("\n"); at >= 0; at = text.indexOf("\n", at + 1)) count += 1;
  return count;
};

/** Every match of `needle`, ignoring case, by offset. Empty needle: none. */
export function findMatches(text: string, needle: string): { start: number; end: number }[] {
  if (!needle) return [];
  const hay = text.toLowerCase();
  const find = needle.toLowerCase();
  const out: { start: number; end: number }[] = [];
  for (let at = hay.indexOf(find); at >= 0; at = hay.indexOf(find, at + find.length)) out.push({ start: at, end: at + find.length });
  return out;
}

export interface Piece {
  text: string;
  color?: string;
  /** The index of the find match this piece is part of. */
  match?: number;
}

/** One line's tokens cut at the match ranges that fall inside it. `from` is the line's offset in the file. */
export function piecesOf(
  tokens: readonly { content: string; color?: string }[],
  from: number,
  matches: readonly { start: number; end: number; id: number }[],
): Piece[] {
  const out: Piece[] = [];
  let at = from;
  for (const token of tokens) {
    const end = at + token.content.length;
    let cursor = at;
    for (const match of matches) {
      if (match.end <= cursor || match.start >= end) continue;
      const a = Math.max(match.start, cursor);
      const b = Math.min(match.end, end);
      if (a > cursor) out.push({ text: token.content.slice(cursor - at, a - at), ...(token.color ? { color: token.color } : {}) });
      out.push({ text: token.content.slice(a - at, b - at), ...(token.color ? { color: token.color } : {}), match: match.id });
      cursor = b;
    }
    if (cursor < end) out.push({ text: token.content.slice(cursor - at), ...(token.color ? { color: token.color } : {}) });
    at = end;
  }
  return out;
}

/** The reason a failed save gives, in plain words. */
export function saveFailureReason(failure: { status: number; code: string; message: string }): string | null {
  if (/permission denied|read-only|read only/i.test(failure.message)) return "The file is read-only.";
  if (/no space|disk full/i.test(failure.message)) return "The disk is full.";
  if (failure.status === 404 || /no such file|not found/i.test(failure.message)) return "The file was deleted.";
  return null;
}
