import type { ExplorerChange } from "../../../generated/ExplorerChange";
import type { FileDiff } from "../../../api/types";
import { CHIP_EXCERPT_LIMIT } from "../../../product/composer/composerContext";

/** A diff over this many lines is cut after its first screen; Show anyway draws the rest. */
export const LINE_LIMIT = 2000;
/** A lock file shorter than this is shown as any other file is. */
export const LOCK_COLLAPSE = 200;

export type DiffLine = { kind: "hunk" | "add" | "del" | "ctx" | "meta"; text: string };

/**
 * The lines of a patch as drawn. The raw git header (`diff --git`, `index`, `---`/`+++`, mode and
 * rename lines) is never shown: everything before the first hunk is dropped, and inside a hunk a
 * line is what its first character says, so a removed line that starts with `--` stays a removal.
 */
export function parsePatch(patch: string): DiffLine[] {
  const lines = patch.split("\n");
  if (lines.at(-1) === "") lines.pop();
  const first = lines.findIndex((text) => text.startsWith("@@"));
  if (first < 0) return [];
  return lines.slice(first).map((text) => {
    if (text.startsWith("@@")) return { kind: "hunk", text };
    if (text.startsWith("+")) return { kind: "add", text };
    if (text.startsWith("-")) return { kind: "del", text };
    if (text.startsWith("\\")) return { kind: "meta", text };
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

/** What a hunk header says: the code it sits in, or the raw range when git names none. */
export function hunkLabel(text: string): string {
  const match = /^@@ [^@]*@@\s?(.*)$/.exec(text);
  return match?.[1] ? `@@ ${match[1]}` : text;
}

export type Row = { line: DiffLine; oldNo: number | null; newNo: number | null; at: number };

/**
 * Each line with its own numbers: removed lines take an old-side number, added lines a new-side one and
 * unchanged lines both. A hunk header restarts both counts; metadata and headers have none.
 */
export function numberRows(lines: readonly DiffLine[]): Row[] {
  let oldNext = 0;
  let newNext = 0;
  return lines.map((line, at) => {
    if (line.kind === "hunk") {
      const match = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(line.text);
      oldNext = match?.[1] ? Number(match[1]) : 0;
      newNext = match?.[2] ? Number(match[2]) : 0;
      return { line, oldNo: null, newNo: null, at };
    }
    if (line.kind === "del") return { line, oldNo: oldNext > 0 ? oldNext++ : null, newNo: null, at };
    if (line.kind === "add") return { line, oldNo: null, newNo: newNext > 0 ? newNext++ : null, at };
    if (line.kind === "ctx") {
      return { line, oldNo: oldNext > 0 ? oldNext++ : null, newNo: newNext > 0 ? newNext++ : null, at };
    }
    return { line, oldNo: null, newNo: null, at };
  });
}

/** The number a row shows in a unified gutter: the old one for a removal, the new one for the rest. */
export const gutterNumber = (row: Row) => (row.line.kind === "del" ? row.oldNo : row.newNo);

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

/** Pairs each run of removals with the additions after it, so a split view has an old and a new side. */
export function splitRows(rows: readonly Row[]): { left: Row | null; right: Row | null }[] {
  const out: { left: Row | null; right: Row | null }[] = [];
  let removed: Row[] = [];
  let added: Row[] = [];
  const flush = () => {
    for (let at = 0; at < Math.max(removed.length, added.length); at += 1) {
      out.push({ left: removed[at] ?? null, right: added[at] ?? null });
    }
    removed = [];
    added = [];
  };
  for (const row of rows) {
    if (row.line.kind === "del") {
      if (added.length > 0) flush();
      removed.push(row);
    } else if (row.line.kind === "add") {
      added.push(row);
    } else {
      flush();
      out.push({ left: row, right: row });
    }
  }
  flush();
  return out;
}

/**
 * The new-side text a diff comment covers: the added and unchanged rows from `start` to `end` (row
 * positions), as shown. Removed lines have no new number and stay out.
 */
export function diffCommentRange(rows: readonly Row[], start: number, end: number) {
  const covered = rows.slice(Math.min(start, end), Math.max(start, end) + 1).filter((row) => row.newNo !== null);
  const first = covered[0]?.newNo;
  const last = covered.at(-1)?.newNo;
  if (first == null || last == null) return null;
  const excerpt = covered.map((row) => row.line.text.slice(1)).join("\n");
  if (excerpt.length > CHIP_EXCERPT_LIMIT) return null;
  return { line: first, endLine: last, excerpt };
}

/** What kind of change a file is, for its letter. A copy is a file that is new here. */
export function changeOf(file: Pick<FileDiff, "status">): ExplorerChange {
  switch (file.status) {
    case "added":
    case "copied":
      return "added";
    case "deleted":
      return "deleted";
    case "renamed":
      return "renamed";
    case "conflicted":
      return "conflicted";
    default:
      return "modified";
  }
}

/** Git says a binary file differs without listing lines. */
export const isBinaryPatch = (patch: string) => /^Binary files .* differ$|^GIT binary patch$/m.test(patch);

/** A rename's old path and similarity, read from the header git wrote for it. */
export function renameOf(patch: string): { from: string; similarity: number | null } | null {
  const header = patch.slice(0, patch.search(/^@@/m) < 0 ? undefined : patch.search(/^@@/m));
  const from = /^rename from (.+)$/m.exec(header)?.[1];
  if (!from) return null;
  const similarity = /^similarity index (\d+)%$/m.exec(header)?.[1];
  return { from, similarity: similarity ? Number(similarity) : null };
}

/** Lock files start collapsed behind the same row as a huge diff; nobody reviews them line by line. */
const LOCK_FILES = new Set([
  "package-lock.json",
  "npm-shrinkwrap.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "bun.lock",
  "bun.lockb",
  "Cargo.lock",
  "Gemfile.lock",
  "poetry.lock",
  "composer.lock",
  "go.sum",
  "Pipfile.lock",
]);
export const isLockFile = (path: string) => LOCK_FILES.has(path.split("/").at(-1) ?? "");

/** One match of the find bar: the file it is in, the row inside that file's shown lines and where in the text. */
type FindHit = { path: string; row: number; start: number };

/** Every occurrence of `needle` in the text of the lines each file shows, in list order. Case does not matter. */
export function findHits(files: readonly { path: string; lines: readonly DiffLine[] }[], needle: string): FindHit[] {
  const wanted = needle.toLowerCase();
  if (wanted === "") return [];
  const hits: FindHit[] = [];
  for (const file of files) {
    file.lines.forEach((line, row) => {
      if (line.kind === "hunk") return;
      const text = line.text.slice(1).toLowerCase();
      for (let at = text.indexOf(wanted); at >= 0; at = text.indexOf(wanted, at + wanted.length)) {
        hits.push({ path: file.path, row, start: at });
      }
    });
  }
  return hits;
}
