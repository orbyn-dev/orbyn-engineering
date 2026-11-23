import { describe, expect, it } from "vitest";
import { capTail, cleanCommand, durationWords, filterBlockLines, findTextMatches, lastLines, rerunText, runningClock, stripTerminalText } from "./blockText";

describe("stripTerminalText", () => {
  it("removes colour, cursor and title sequences and keeps the words", () => {
    expect(stripTerminalText("\u001b[31mred\u001b[0m \u001b[2K\u001b[1Gdone\u001b]0;title\u0007!")).toBe("red done!");
    expect(stripTerminalText("a\u001b]8;;http://x\u001b\\link\u001b]8;;\u001b\\b")).toBe("alinkb");
  });

  it("keeps the last text of a line a carriage return redrew, and joins CRLF", () => {
    expect(stripTerminalText("10%\r50%\r100%\nnext\r\nend")).toBe("100%\nnext\nend");
  });

  it("drops other control characters but keeps tabs and newlines", () => {
    expect(stripTerminalText("a\u0000b\u0007c\td\ne\u007ff")).toBe("abc\td\nef");
  });

  it("leaves markup as the characters it is", () => {
    expect(stripTerminalText("<img src=x onerror=alert(1)>")).toBe("<img src=x onerror=alert(1)>");
  });
});

describe("capTail", () => {
  it("keeps the end of a long text and says it cut", () => {
    expect(capTail("abcdef", 3)).toEqual({ text: "def", cut: true });
    expect(capTail("abc", 3)).toEqual({ text: "abc", cut: false });
  });
});

describe("cleanCommand", () => {
  it("makes one capped line and gives null for nothing", () => {
    expect(cleanCommand("  npm\u001b[0m   test \n")).toBe("npm test");
    expect(cleanCommand(null)).toBeNull();
    expect(cleanCommand("   ")).toBeNull();
    expect(cleanCommand("x".repeat(5000))).toHaveLength(4096);
  });
});

describe("rerunText", () => {
  it("gives the command as written", () => {
    expect(rerunText("npm test -- --run")).toBe("npm test -- --run");
    expect(rerunText("echo\ta")).toBe("echo\ta");
  });
  it("refuses a command it would have to repair: empty, a newline, a control character, too long", () => {
    expect(rerunText(null)).toBeNull();
    expect(rerunText("  ")).toBeNull();
    expect(rerunText("ls\nrm -rf x")).toBeNull();
    expect(rerunText("ls\r")).toBeNull();
    expect(rerunText("ls\u001b[2J")).toBeNull();
    expect(rerunText("x".repeat(4097))).toBeNull();
  });
});

describe("durationWords", () => {
  const at = (ms: number) => new Date(Date.UTC(2026, 10, 22, 14, 0, 0) + ms).toISOString();
  it("says tenths under ten seconds, then whole seconds, minutes and hours", () => {
    expect(durationWords(at(0), at(100))).toBe("0.1 s");
    expect(durationWords(at(0), at(4_240))).toBe("4.2 s");
    expect(durationWords(at(0), at(38_400))).toBe("38 s");
    expect(durationWords(at(0), at(72_000))).toBe("1 m 12 s");
    expect(durationWords(at(0), at(3_720_000))).toBe("1 h 02 m");
  });
  it("gives an empty text for a date it cannot read, and never a negative time", () => {
    expect(durationWords("nope", at(0))).toBe("");
    expect(durationWords(at(5_000), at(0))).toBe("0 s");
  });
});

describe("runningClock", () => {
  it("counts minutes and seconds, then hours", () => {
    const start = new Date(Date.UTC(2026, 10, 22, 14, 0, 0)).toISOString();
    const at = (s: number) => Date.UTC(2026, 10, 22, 14, 0, 0) + s * 1000;
    expect(runningClock(start, at(42))).toBe("0:42");
    expect(runningClock(start, at(62))).toBe("1:02");
    expect(runningClock(start, at(3723))).toBe("1:02:03");
    expect(runningClock("nope", at(1))).toBe("");
  });
});

describe("lastLines", () => {
  it("returns the last lines and the count, ignoring one final newline", () => {
    expect(lastLines("a\nb\nc\n", 2)).toEqual({ text: "b\nc", total: 3, cut: true });
    expect(lastLines("a\nb", 6)).toEqual({ text: "a\nb", total: 2, cut: false });
    expect(lastLines("", 6)).toEqual({ text: "", total: 0, cut: false });
  });
});

describe("filterBlockLines", () => {
  const seven = "line one\nFAIL here\nline three\nline four\nline five\nline six\nline seven";

  it("keeps only the lines that contain the typed text, ignoring case", () => {
    expect(filterBlockLines(seven, "fail")).toEqual({ text: "FAIL here", total: 7, kept: 1 });
    expect(filterBlockLines(seven, "FAIL")).toEqual({ text: "FAIL here", total: 7, kept: 1 });
  });

  it("keeps every line for an empty query, and counts the seven-line example", () => {
    expect(filterBlockLines(seven, "")).toEqual({ text: seven, total: 7, kept: 7 });
  });

  it("treats regex characters as plain text", () => {
    expect(filterBlockLines("a.b\naxb\n[a]\na+b", "a.b")).toEqual({ text: "a.b", total: 4, kept: 1 });
    expect(filterBlockLines("a.b\naxb\n[a]\na+b", "[a]")).toEqual({ text: "[a]", total: 4, kept: 1 });
    expect(filterBlockLines("a.b\naxb\n[a]\na+b", "a+b")).toEqual({ text: "a+b", total: 4, kept: 1 });
  });

  it("keeps the last line as a line and does not count one final newline", () => {
    expect(filterBlockLines("a\nb\n", "b")).toEqual({ text: "b", total: 2, kept: 1 });
    expect(filterBlockLines("a\nb\n", "")).toEqual({ text: "a\nb", total: 2, kept: 2 });
    expect(filterBlockLines("", "x")).toEqual({ text: "", total: 0, kept: 0 });
  });

  it("says none remain when nothing matches", () => {
    expect(filterBlockLines(seven, "absent")).toEqual({ text: "", total: 7, kept: 0 });
  });
});

describe("findTextMatches", () => {
  it("finds every match, ignoring case", () => {
    expect(findTextMatches("Next next NEXT", "next")).toEqual([
      { start: 0, end: 4 },
      { start: 5, end: 9 },
      { start: 10, end: 14 },
    ]);
  });

  it("finds adjacent matches without counting one overlapping match twice", () => {
    expect(findTextMatches("abab", "ab")).toEqual([
      { start: 0, end: 2 },
      { start: 2, end: 4 },
    ]);
    expect(findTextMatches("aaa", "aa")).toEqual([{ start: 0, end: 2 }]);
    expect(findTextMatches("aaaa", "aa")).toEqual([
      { start: 0, end: 2 },
      { start: 2, end: 4 },
    ]);
  });

  it("gives nothing for an empty query or text that is not there", () => {
    expect(findTextMatches("any text", "")).toEqual([]);
    expect(findTextMatches("any text", "absent")).toEqual([]);
    expect(findTextMatches("", "x")).toEqual([]);
  });

  it("treats regex characters as plain text", () => {
    expect(findTextMatches("a.b axb a+b", ".")).toEqual([{ start: 1, end: 2 }]);
    expect(findTextMatches("a[b] c", "[")).toEqual([{ start: 1, end: 2 }]);
    expect(findTextMatches("a[b] c", "a[b]")).toEqual([{ start: 0, end: 4 }]);
  });
});
