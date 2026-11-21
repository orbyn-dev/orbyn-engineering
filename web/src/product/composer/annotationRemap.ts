import type { ComposerChip, ReviewChip } from "./composerContext";

/**
 * Keeps a file comment pointing at the same text after the file is edited. The old and new text are compared line by line;
 * a comment follows its lines only when every one of them is still there, unchanged and in one block. If any line was
 * edited, deleted or split by an insert, the comment is stale: it keeps its old position and excerpt and says so, and is
 * never moved to different text.
 */
export type Remapped = { stale: false; line: number; endLine: number } | { stale: true };

/** The most table cells the line match may use; a larger edit falls back to the unchanged head and tail only. */
const MATCH_CELLS = 10_000_000;

/** For each old line (0-based) the new line (0-based) it still is, or -1. */
function matchLines(before: readonly string[], after: readonly string[]): Int32Array {
  const map = new Int32Array(before.length).fill(-1);
  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) {
    map[head] = head;
    head++;
  }
  let tail = 0;
  while (tail < before.length - head && tail < after.length - head
    && before[before.length - 1 - tail] === after[after.length - 1 - tail]) {
    map[before.length - 1 - tail] = after.length - 1 - tail;
    tail++;
  }
  const n = before.length - head - tail;
  const m = after.length - head - tail;
  if (n === 0 || m === 0 || (n + 1) * (m + 1) > MATCH_CELLS || Math.min(n, m) >= 65535) return map;
  // Longest common subsequence of the changed middle: table[i][j] is its length for before[i..] and after[j..].
  const width = m + 1;
  const table = new Uint16Array((n + 1) * width);
  const cell = (row: number, col: number) => table[row * width + col] ?? 0;
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * width + j] = before[head + i] === after[head + j]
        ? cell(i + 1, j + 1) + 1
        : Math.max(cell(i + 1, j), cell(i, j + 1));
    }
  }
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (before[head + i] === after[head + j]) { map[head + i] = head + j; i++; j++; }
    else if (cell(i + 1, j) >= cell(i, j + 1)) i++;
    else j++;
  }
  return map;
}

/** Where a one-based line range of `before` is in `after`, or stale when its text is no longer there as it was. */
export function remapAnnotation(before: string, after: string, range: { line: number; endLine?: number }): Remapped {
  const old = before.split("\n");
  const next = after.split("\n");
  const first = range.line;
  const last = range.endLine ?? range.line;
  if (!Number.isSafeInteger(first) || !Number.isSafeInteger(last) || first < 1 || last < first || last > old.length) return { stale: true };
  const map = matchLines(old, next);
  const start = map[first - 1] ?? -1;
  if (start < 0) return { stale: true };
  for (let row = first; row <= last; row++) {
    if (map[row - 1] !== start + (row - first)) return { stale: true };
  }
  return { stale: false, line: start + 1, endLine: start + 1 + (last - first) };
}

export interface RemapWhere {
  cwd: string;
  path: string;
  fromRevision: string;
  toRevision: string;
  before: string;
  after: string;
}

/**
 * The file comments of one saved file, moved to its new revision. Comments for other files, checkouts or revisions are
 * returned untouched, and the same array comes back when none applies.
 */
export function remapReviewChips(chips: readonly ComposerChip[], where: RemapWhere): ComposerChip[] {
  let changed = false;
  const result = chips.map((chip): ComposerChip => {
    if (chip.kind !== "review" || chip.line === null || chip.path !== where.path
      || chip.sourceCwd !== where.cwd || chip.sourceRevision !== where.fromRevision) return chip;
    changed = true;
    const { stale: _was, ...rest } = chip;
    // A comment already out of date stays so: later edits never bring it back to some other text.
    if (chip.stale) return { ...rest, stale: true, sourceRevision: where.toRevision } satisfies ReviewChip;
    const moved = remapAnnotation(where.before, where.after, { line: chip.line, endLine: chip.endLine });
    if (moved.stale) return { ...rest, stale: true, sourceRevision: where.toRevision } satisfies ReviewChip;
    return { ...rest, line: moved.line, ...(chip.endLine === undefined ? {} : { endLine: moved.endLine }), sourceRevision: where.toRevision };
  });
  return changed ? result : (chips as ComposerChip[]);
}
