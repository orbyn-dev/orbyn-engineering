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

it("keeps a file's CRLF endings and a missing final newline", () => {
  const windows = NOW.replace(/\n/g, "\r\n");
  expect(revertedContent(windows, hunks[0]!)).toBe("a\r\nb\r\nold\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\nx\r\ntail\r\n");
  const single = hunksOf(numberRows(parsePatch("@@ -1,2 +1,2 @@\n-one\n+two\n three\n")))[0]!;
  expect(revertedContent("two\nthree", single)).toBe("one\nthree");
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
