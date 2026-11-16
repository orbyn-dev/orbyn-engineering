import { expect, it } from "vitest";
import {
  changeOf, diffCommentRange, findHits, gutterNumber, hunkLabel, isBinaryPatch, isLockFile, numberRows, parsePatch, renameOf,
  splitRows, withoutWhitespaceChanges,
} from "./diffModel";

const PATCH = "diff --git a/n.txt b/n.txt\nindex 1..2 100644\n--- a/n.txt\n+++ b/n.txt\n@@ -1,3 +1,3 @@ fn()\n one\n-two\n+TWO\n three\n";

it("numbers a removal on the old side, an addition on the new side and a context line on both", () => {
  const rows = numberRows(parsePatch(PATCH));
  expect(rows.map((row) => [row.line.kind, row.oldNo, row.newNo])).toEqual([
    ["hunk", null, null], ["ctx", 1, 1], ["del", 2, null], ["add", null, 2], ["ctx", 3, 3],
  ]);
  expect(rows.map(gutterNumber)).toEqual([null, 1, 2, 2, 3]);
  expect(rows.map((row) => row.at)).toEqual([0, 1, 2, 3, 4]);
});

it("names a hunk by the code it sits in, or by its range when git names none", () => {
  expect(hunkLabel("@@ -1,3 +1,3 @@ fn()")).toBe("@@ fn()");
  expect(hunkLabel("@@ -0,0 +1,26 @@")).toBe("@@ -0,0 +1,26 @@");
});

it("pairs removals with the additions after them for a split view", () => {
  const pairs = splitRows(numberRows(parsePatch(PATCH)));
  expect(pairs.map((pair) => [pair.left?.line.kind ?? null, pair.right?.line.kind ?? null])).toEqual([
    ["hunk", "hunk"], ["ctx", "ctx"], ["del", "add"], ["ctx", "ctx"],
  ]);
});

it("a comment range covers the shown new-side rows, skips removals, and stays within the chip limit", () => {
  const rows = numberRows(parsePatch(PATCH));
  expect(diffCommentRange(rows, 3, 1)).toEqual({ line: 1, endLine: 2, excerpt: "one\nTWO" });
  expect(diffCommentRange(rows, 2, 2)).toBeNull();
  const long = numberRows(parsePatch(`@@ -0,0 +1 @@\n+${"x".repeat(5000)}\n`));
  expect(diffCommentRange(long, 0, 1)).toBeNull();
});

it("drops a pair that differs only in spacing, as git diff -w would", () => {
  const lines = withoutWhitespaceChanges(parsePatch("@@ -1 +1 @@\n-a  b\n+a b\n"));
  expect(lines.map((line) => line.kind)).toEqual(["hunk", "ctx"]);
});

it("reads a rename's old path and similarity from its header, and knows binary patches and lock files", () => {
  expect(renameOf("similarity index 94%\nrename from a/b.ts\nrename to c.ts\n@@ -1 +1 @@\n-x\n+y\n")).toEqual({ from: "a/b.ts", similarity: 94 });
  expect(renameOf("@@ -1 +1 @@\n-rename from nowhere\n+x\n")).toBeNull();
  expect(isBinaryPatch("diff --git a/x.png b/x.png\nBinary files a/x.png and b/x.png differ\n")).toBe(true);
  expect(isBinaryPatch("@@ -1 +1 @@\n-a\n+b\n")).toBe(false);
  expect(isLockFile("web/package-lock.json")).toBe(true);
  expect(isLockFile("src/lock.ts")).toBe(false);
});

it("maps a file's status to its letter's kind", () => {
  expect(["added", "copied", "deleted", "renamed", "conflicted", "modified", "weird"].map((status) => changeOf({ status }))).toEqual([
    "added", "added", "deleted", "renamed", "conflicted", "modified", "modified",
  ]);
});

it("finds every occurrence, in list order and without regard to case, never in a hunk header", () => {
  const files = [
    { path: "a", lines: parsePatch("@@ -1 +1 @@ next()\n-Next next\n+x\n") },
    { path: "b", lines: parsePatch("@@ -1 +1 @@\n y next\n") },
  ];
  expect(findHits(files, "next")).toEqual([
    { path: "a", row: 1, start: 0 }, { path: "a", row: 1, start: 5 }, { path: "b", row: 1, start: 2 },
  ]);
  expect(findHits(files, "")).toEqual([]);
});
