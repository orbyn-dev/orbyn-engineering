import { CHIP_EXCERPT_LIMIT } from "../../../product/composer/composerContext";

export const LINE_LIMIT = 2000;
/** A large diff renders files in pages, so a huge change set never mounts at once. */
const FILE_PAGE = 50;
const PAGE_LINES = 4000;

/** Where the page that starts at `from` ends: 50 files or about 4,000 patch lines, at least one file. */
export function nextPageEnd(files: readonly { patch: string }[], from: number): number {
  let lines = 0;
  let at = from;
  while (at < files.length && at - from < FILE_PAGE) {
    lines += files[at]!.patch.split("\n").length;
    if (at > from && lines > PAGE_LINES) break;
    at += 1;
  }
  return at;
}

export type DiffLine = { kind: "hunk" | "add" | "del" | "ctx" | "meta"; text: string };

export function parsePatch(patch: string): DiffLine[] {
  const lines = patch.split("\n");
  if (lines.at(-1) === "") lines.pop();
  return lines.map((text) => {
    if (text.startsWith("@@")) return { kind: "hunk", text };
    if (text.startsWith("+++") || text.startsWith("---")) return { kind: "meta", text };
    if (text.startsWith("+")) return { kind: "add", text };
    if (text.startsWith("-")) return { kind: "del", text };
    if (text.startsWith("\\") || text.startsWith("diff ") || text.startsWith("index ")) {
      return { kind: "meta", text };
    }
    return { kind: "ctx", text };
  });
}

export const markers: Record<DiffLine["kind"], string | null> = {
  add: "Added",
  del: "Removed",
  hunk: null,
  ctx: null,
  meta: null,
};

/**
 * The new-side line number each line lands on. A hunk header resets the count;
 * removed lines take no new number, and metadata has none.
 */
export function numberedLines(lines: readonly DiffLine[]): { line: DiffLine; number: number | null }[] {
  let next = 0;
  return lines.map((line) => {
    if (line.kind === "hunk") {
      const match = /^@@ -\d+(?:,\d+)? \+(\d+)/.exec(line.text);
      next = match?.[1] ? Number(match[1]) : 0;
      return { line, number: null };
    }
    if (line.kind === "add" || line.kind === "ctx") {
      const number = next > 0 ? next : null;
      if (next > 0) next += 1;
      return { line, number };
    }
    return { line, number: null };
  });
}

/** Drops changed pairs that differ only in spacing, as `git diff -w` would show them. */
export function withoutWhitespaceChanges(lines: DiffLine[]): DiffLine[] {
  const squash = (text: string) => text.slice(1).replace(/\s+/g, "");
  const out: DiffLine[] = [];
  for (let at = 0; at < lines.length; at += 1) {
    const line = lines[at];
    const next = lines[at + 1];
    if (line?.kind === "del" && next?.kind === "add" && squash(line.text) === squash(next.text)) {
      out.push({ kind: "ctx", text: ` ${next.text.slice(1)}` });
      at += 1;
    } else if (line) {
      out.push(line);
    }
  }
  return out;
}

export type Numbered = { line: DiffLine; number: number | null };

/** Pairs each run of removals with the additions after it, keeping new-side numbers. */
export function splitNumbered(entries: readonly Numbered[]): { left: Numbered | null; right: Numbered | null }[] {
  const rows: { left: Numbered | null; right: Numbered | null }[] = [];
  let removed: Numbered[] = [];
  let added: Numbered[] = [];
  const flush = () => {
    for (let at = 0; at < Math.max(removed.length, added.length); at += 1) {
      rows.push({ left: removed[at] ?? null, right: added[at] ?? null });
    }
    removed = [];
    added = [];
  };
  for (const entry of entries) {
    if (entry.line.kind === "del") {
      if (added.length > 0) flush();
      removed.push(entry);
    } else if (entry.line.kind === "add") {
      added.push(entry);
    } else {
      flush();
      rows.push({ left: entry, right: entry });
    }
  }
  flush();
  return rows;
}

/**
 * The new-side text a diff comment covers: the added and unchanged lines from
 * `start` to `end`, as shown. Removed lines have no new number and stay out.
 */
export function diffCommentRange(entries: readonly Numbered[], start: number, end: number) {
  const line = Math.min(start, end);
  const endLine = Math.max(start, end);
  const covered = entries.filter((entry) => entry.number !== null && entry.number >= line && entry.number <= endLine);
  if (covered.length === 0) return null;
  const excerpt = covered.map((entry) => entry.line.text.slice(1)).join("\n");
  if (excerpt.length > CHIP_EXCERPT_LIMIT) return null;
  return { line, endLine, excerpt };
}
