import { describe, expect, it } from "vitest";
import { remapAnnotation, remapReviewChips } from "./annotationRemap";
import type { ComposerChip } from "./composerContext";

const lines = (...rows: string[]) => rows.join("\n");
const base = lines("a", "b", "c", "d", "e", "f");

describe("remapAnnotation", () => {
  const cases: Array<{ name: string; before: string; after: string; line: number; endLine: number; want: { line: number; endLine: number } | "stale" }> = [
    { name: "unchanged file", before: base, after: base, line: 3, endLine: 4, want: { line: 3, endLine: 4 } },
    { name: "insert above shifts down", before: base, after: lines("new1", "new2", ...base.split("\n")), line: 3, endLine: 3, want: { line: 5, endLine: 5 } },
    { name: "insert below leaves it", before: base, after: lines(base, "g"), line: 2, endLine: 3, want: { line: 2, endLine: 3 } },
    { name: "delete above shifts up", before: base, after: lines("c", "d", "e", "f"), line: 4, endLine: 4, want: { line: 2, endLine: 2 } },
    { name: "delete a range below leaves it", before: base, after: lines("a", "b", "c"), line: 2, endLine: 2, want: { line: 2, endLine: 2 } },
    { name: "delete the anchored line is stale", before: base, after: lines("a", "b", "d", "e", "f"), line: 3, endLine: 3, want: "stale" },
    { name: "delete part of the range is stale", before: base, after: lines("a", "b", "d", "e", "f"), line: 3, endLine: 4, want: "stale" },
    { name: "edit inside the range is stale", before: base, after: lines("a", "b", "c", "D!", "e", "f"), line: 3, endLine: 5, want: "stale" },
    { name: "edit the anchor line is stale", before: base, after: lines("a", "b", "C!", "d", "e", "f"), line: 3, endLine: 3, want: "stale" },
    { name: "edit beside the anchor keeps it", before: base, after: lines("a", "B!", "c", "D!", "e", "f"), line: 3, endLine: 3, want: { line: 3, endLine: 3 } },
    { name: "insert inside a range is stale", before: base, after: lines("a", "b", "c", "x", "d", "e", "f"), line: 3, endLine: 4, want: "stale" },
    { name: "multi-edit: insert above and delete below", before: base, after: lines("top", "a", "b", "c", "d"), line: 3, endLine: 4, want: { line: 4, endLine: 5 } },
    { name: "multi-edit: insert above and edit the anchor", before: base, after: lines("top", "a", "b", "C!", "d", "e", "f"), line: 3, endLine: 3, want: "stale" },
    { name: "range shifts as a block", before: base, after: lines("a", "x", "y", "b", "c", "d", "e", "f"), line: 2, endLine: 4, want: { line: 4, endLine: 6 } },
    { name: "a line moved away is stale, never re-pointed", before: lines("a", "b", "c"), after: lines("c", "a", "b", "z"), line: 3, endLine: 3, want: "stale" },
    { name: "out of range annotation is stale", before: base, after: base, line: 9, endLine: 9, want: "stale" },
  ];
  for (const row of cases) {
    it(row.name, () => {
      const got = remapAnnotation(row.before, row.after, { line: row.line, endLine: row.endLine });
      if (row.want === "stale") expect(got).toEqual({ stale: true });
      else expect(got).toEqual({ stale: false, ...row.want });
    });
  }

  it("never points at text that differs from what was annotated", () => {
    const before = lines("x", "same", "y");
    const after = lines("x", "other", "y");
    expect(remapAnnotation(before, after, { line: 2, endLine: 2 })).toEqual({ stale: true });
  });

  it("matches the right copy of a repeated line", () => {
    const before = lines("}", "a", "}", "b", "}");
    const after = lines("}", "a", "new", "}", "b", "}");
    const got = remapAnnotation(before, after, { line: 3, endLine: 3 });
    expect(got).toEqual({ stale: false, line: 4, endLine: 4 });
  });

  it("still answers for a very large file", () => {
    const big = Array.from({ length: 6000 }, (_, i) => `row ${i}`);
    const edited = ["inserted", ...big.slice(0, 3000), "changed", ...big.slice(3001)];
    expect(remapAnnotation(big.join("\n"), edited.join("\n"), { line: 10, endLine: 10 })).toEqual({ stale: false, line: 11, endLine: 11 });
    expect(remapAnnotation(big.join("\n"), edited.join("\n"), { line: 3001, endLine: 3001 })).toEqual({ stale: true });
  });
});

describe("remapReviewChips", () => {
  const review = (over: Partial<Extract<ComposerChip, { kind: "review" }>>): ComposerChip => ({
    id: "r1", kind: "review", path: "a.ts", line: 3, endLine: 3, comment: "why", excerpt: "c",
    sourceRevision: "rev1", sourceCwd: "/w", ...over,
  });
  const run = (chips: ComposerChip[], before = base, after = lines("top", ...base.split("\n"))) =>
    remapReviewChips(chips, { cwd: "/w", path: "a.ts", fromRevision: "rev1", toRevision: "rev2", before, after });

  it("moves the line and the revision, keeps the comment and excerpt", () => {
    const [moved] = run([review({})]);
    expect(moved).toMatchObject({ line: 4, endLine: 4, sourceRevision: "rev2", comment: "why", excerpt: "c" });
    expect(moved).not.toHaveProperty("stale");
  });
  it("marks a changed anchor stale on the new revision with its old line", () => {
    const [chip] = run([review({})], base, lines("a", "b", "C!", "d", "e", "f"));
    expect(chip).toMatchObject({ stale: true, sourceRevision: "rev2", line: 3, excerpt: "c" });
  });
  it("keeps a stale comment stale through later edits", () => {
    const [chip] = run([review({ stale: true })], base, base);
    expect(chip).toMatchObject({ stale: true, line: 3, sourceRevision: "rev2" });
  });
  it("leaves other files, checkouts, revisions and kinds alone", () => {
    const others = [review({ id: "o1", path: "b.ts" }), review({ id: "o2", sourceCwd: "/x" }), review({ id: "o3", sourceRevision: "old" }), review({ id: "o4", line: null, endLine: undefined })];
    expect(run(others)).toEqual(others);
  });
  it("returns the same array when nothing applies", () => {
    const none = [review({ path: "z.ts" })];
    expect(run(none)).toBe(none);
  });
  it("moves only the comments of the repository whose file was saved", () => {
    const mine = review({ id: "mine", repoId: "planner", repoName: "orbyn-planner" });
    const primary = review({ id: "primary" });
    const other = review({ id: "other", repoId: "docs" });
    const before = base;
    const after = lines("top", ...base.split("\n"));
    const moved = remapReviewChips([mine, primary, other], { cwd: "/w", path: "a.ts", repoId: "planner", fromRevision: "rev1", toRevision: "rev2", before, after });
    expect(moved[0]).toMatchObject({ line: 4, endLine: 4, sourceRevision: "rev2" });
    expect(moved[1]).toBe(primary);
    expect(moved[2]).toBe(other);
  });
});
