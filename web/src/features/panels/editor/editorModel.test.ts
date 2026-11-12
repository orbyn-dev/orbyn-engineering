import { describe, expect, it } from "vitest";
import { toLF, withEol, endingsOf, endingLabel, findMatches, indentOf, languageOf, lineCount, lineStart, piecesOf, positionOf, saveFailureReason } from "./editorModel";

describe("editor model", () => {
  it("names the language from the file name and has no grammar for plain text", () => {
    expect(languageOf("auth/redirect.ts")).toEqual({ label: "TypeScript", grammar: "typescript" });
    expect(languageOf("data/export.csv")).toEqual({ label: "CSV", grammar: null });
    expect(languageOf("Makefile")).toEqual({ label: "Plain text", grammar: null });
  });

  it("reads the line ending, whether it is mixed, and the indent from the text", () => {
    expect(endingsOf("a\r\nb")).toEqual({ eol: "CRLF", mixed: false });
    expect(endingsOf("a\r\nb\r\n")).toEqual({ eol: "CRLF", mixed: false });
    expect(endingsOf("a\nb\n")).toEqual({ eol: "LF", mixed: false });
    expect(endingsOf("a\rb\rc")).toEqual({ eol: "CR", mixed: false });
    expect(endingsOf("a\r\nb\r\nc\nd\r")).toEqual({ eol: "CRLF", mixed: true });
    expect(endingsOf("a\nb\r\n")).toEqual({ eol: "LF", mixed: true });
    expect(endingsOf("no break")).toEqual({ eol: "LF", mixed: false });
    expect(endingLabel("CRLF", true)).toBe("CRLF (mixed)");
    expect(withEol("a\nb", "CR")).toBe("a\rb");
    expect(indentOf("a\n  b\n  c\n    d")).toBe("2 spaces");
    expect(indentOf("a\n    b\n    c")).toBe("4 spaces");
    expect(indentOf("a\n\tb\n\tc")).toBe("Tabs");
    expect(indentOf("one line")).toBe("2 spaces");
  });

  it("converts between offsets and one-based lines and columns", () => {
    const text = "ab\ncde\n\nf";
    expect(positionOf(text, 0)).toEqual({ line: 1, col: 1 });
    expect(positionOf(text, 4)).toEqual({ line: 2, col: 2 });
    expect(positionOf(text, text.length)).toEqual({ line: 4, col: 2 });
    expect(lineStart(text, 3)).toBe(7);
    expect(lineStart(text, 99)).toBe(text.length);
    expect(lineCount(text)).toBe(4);
  });

  it("finds every match, ignoring case, and none for an empty needle", () => {
    expect(findMatches("Next next NEXT", "next")).toEqual([
      { start: 0, end: 4 },
      { start: 5, end: 9 },
      { start: 10, end: 14 },
    ]);
    expect(findMatches("abc", "")).toEqual([]);
  });

  it("cuts a line's tokens at the matches that fall inside it", () => {
    const pieces = piecesOf([{ content: "const next", color: "k" }, { content: " = 1" }], 10, [{ start: 16, end: 20, id: 2 }]);
    expect(pieces.map((piece) => [piece.text, piece.match])).toEqual([
      ["const ", undefined],
      ["next", 2],
      [" = 1", undefined],
    ]);
    expect(pieces[0]?.color).toBe("k");
  });

  it("holds line breaks as line feeds and puts the file's own ending back", () => {
    expect(toLF("a\r\nb\rc\n")).toBe("a\nb\nc\n");
    expect(withEol("a\nb\n", "CRLF")).toBe("a\r\nb\r\n");
    expect(withEol("a\nb\n", "LF")).toBe("a\nb\n");
    expect(withEol(toLF("a\r\nb\r\n"), endingsOf("a\r\nb\r\n").eol)).toBe("a\r\nb\r\n");
  });

  it("puts a failed save in plain words", () => {
    expect(saveFailureReason({ status: 500, code: "io", message: "Permission denied (os error 13)" })).toBe("The file is read-only.");
    expect(saveFailureReason({ status: 500, code: "io", message: "No space left on device" })).toBe("The disk is full.");
    expect(saveFailureReason({ status: 404, code: "not_found", message: "x" })).toBe("The file was deleted.");
    expect(saveFailureReason({ status: 500, code: "io", message: "boom" })).toBeNull();
  });
});
