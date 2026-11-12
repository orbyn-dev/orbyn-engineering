import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, it } from "vitest";
import { numberRows, parsePatch } from "./diffModel";
import { canRevert, changedLines, editedContent, hunkPatch, hunksOf, revertedContent } from "./hunkModel";

const patch = "@@ -2,3 +2,4 @@ after(req)\n b\n-old\n+new one\n+new two\n c\n@@ -10,2 +11,1 @@\n x\n-gone\n";
const hunks = hunksOf(numberRows(parsePatch(patch)));
const NOW = "a\nb\nnew one\nnew two\nc\nd\ne\nf\ng\nh\nx\ntail\n";

it("reads each hunk with its header numbers and the lines it changes", () => {
  expect(hunks).toHaveLength(2);
  expect(hunks.map((hunk) => [hunk.newStart, hunk.newCount])).toEqual([[2, 4], [11, 1]]);
  expect(changedLines(hunks[0]!)).toBe("Lines 3 to 4");
  expect(changedLines(hunks[1]!)).toBe("Line 11");
  expect(hunkPatch(hunks[1]!)).toBe("@@ -10,2 +11,1 @@\n x\n-gone");
});

it("puts a hunk back to the lines the last commit has, and leaves the rest of the file alone", () => {
  expect(revertedContent(NOW, hunks[0]!)).toBe("a\nb\nold\nc\nd\ne\nf\ng\nh\nx\ntail\n");
  expect(revertedContent(NOW, hunks[1]!)).toBe("a\nb\nnew one\nnew two\nc\nd\ne\nf\ng\nh\nx\ngone\ntail\n");
});

it("refuses a hunk when the file no longer holds the lines the diff showed", () => {
  expect(revertedContent(NOW.replace("new one", "changed"), hunks[0]!)).toBeNull();
  expect(revertedContent("a\n", hunks[0]!)).toBeNull();
});

it("keeps the last line's missing final newline", () => {
  const single = hunksOf(numberRows(parsePatch("@@ -1,2 +1,2 @@\n-one\n+two\n three\n")))[0]!;
  expect(revertedContent("two\nthree", single)).toBe("one\nthree");
});

it("puts back a CRLF file from real git output, each line with its own ending", () => {
  const dir = mkdtempSync(join(tmpdir(), "hunk-crlf-"));
  const git = (...args: string[]) => execFileSync("git", ["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", ...args], { cwd: dir, encoding: "utf8" });
  try {
    git("init", "-q");
    // The committed file mixes endings, so a restored line has to keep its own.
    const committed = "a\r\nb\nc\r\nd\r\n";
    writeFileSync(join(dir, "f.txt"), committed);
    git("add", ".");
    git("commit", "-qm", "base");
    writeFileSync(join(dir, "f.txt"), "a\r\nB\r\nB2\r\nd\r\n");
    const patch = git("diff", "HEAD");
    expect(patch).toContain("\r");
    const [hunk] = hunksOf(numberRows(parsePatch(patch)));
    expect(revertedContent("a\r\nB\r\nB2\r\nd\r\n", hunk!)).toBe(committed);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

it("an emptied file's +0,0 hunk comes back only into an empty file", () => {
  const hunk = hunksOf(numberRows(parsePatch("@@ -1,2 +0,0 @@\n-one\n-two\n")))[0]!;
  expect(revertedContent("", hunk)).toBe("one\ntwo\n");
  expect(revertedContent("something\n", hunk)).toBeNull();
});

it("offers no revert for a hunk with a no-newline mark or nothing changed", () => {
  const marked = hunksOf(numberRows(parsePatch("@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n")))[0]!;
  expect(canRevert(marked)).toBe(false);
  expect(canRevert(hunks[0]!)).toBe(true);
});

it("changes one line only while it still reads what the diff showed", () => {
  expect(editedContent("a\nb\r\nc\n", 2, "b", "B")).toBe("a\nB\r\nc\n");
  expect(editedContent("a\nb\nc\n", 2, "other", "B")).toBeNull();
  expect(editedContent("a\n", 5, "b", "B")).toBeNull();
});
