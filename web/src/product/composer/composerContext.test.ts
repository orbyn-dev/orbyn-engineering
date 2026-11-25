import { describe, expect, it } from "vitest";
import {
  CHIP_ATTRIBUTE,
  MAX_PROMPT_CHARS,
  asChip,
  charactersOver,
  chipLabel,
  chipPrompt,
  chipsClipboard,
  chipsFromClipboard,
  fileExcerpt,
  languageForPath,
  promptWithChips,
  refillExcerpts,
  terminalSelection,
  type ComposerChip,
} from "./composerContext";

const chip = (over: Record<string, unknown> & Pick<ComposerChip, "kind">): ComposerChip =>
  ({
    id: "c1",
    path: "src/main.rs",
    line: 12,
    excerpt: "let a = 1;",
    language: "rust",
    title: "Terminal selection",
    output: "cargo test\nok",
    lineStart: 2,
    lineEnd: 3,
    comment: "This line is wrong.",
    url: "http://localhost:5173/",
    selector: "#save",
    text: "Clicked save",
    html: "<button id=\"save\">Save</button>",
    threadId: "t1",
    ...over,
  }) as unknown as ComposerChip;

describe("composer context", () => {
  it("keeps a diff selection's range and its way back to the diff through the clipboard", () => {
    const chip = asChip({ kind: "file", path: "a.ts", line: 2, endLine: 4, fromDiff: true, excerpt: "x", language: "typescript" }, "f")!;
    expect(chipLabel(chip)).toBe("a.ts:2–4");
    expect(chipPrompt(chip)).toContain("File `a.ts:2-4`");
    expect(chipsFromClipboard({ json: chipsClipboard([chip])!.json, html: "" })?.[0]).toMatchObject({ endLine: 4, fromDiff: true });
  });

  it("carries review ranges through the clipboard and preserves legacy one-line comments", () => {
    const ranged = asChip({ kind: "review", path: "a.ts", line: 2, endLine: 4, comment: "why?", excerpt: "captured",
      sourceRevision: "r", sourceCwd: "/project" }, "r")!;
    expect(chipLabel(ranged)).toBe("a.ts:2–4");
    expect(chipPrompt(ranged)).toContain("Review on `a.ts:2–4`\nwhy?");
    const clipboard = chipsClipboard([ranged])!;
    expect(chipsFromClipboard({ json: clipboard.json, html: "" })?.[0]).toMatchObject({
      line: 2, endLine: 4, excerpt: "captured", sourceRevision: "r", sourceCwd: "/project",
    });
    expect(asChip({ kind: "review", path: "a.ts", line: 2, comment: "old", excerpt: null })).toMatchObject({ line: 2 });
    for (const endLine of [0, -1, 1, 2.2, "4"]) {
      expect(asChip({ kind: "review", path: "a.ts", line: 2, endLine, comment: "bad" })).toBeNull();
    }
  });

  it("names a review comment's repository, kept through the clipboard", () => {
    const chip = asChip({ kind: "review", path: "notes.txt", line: 2, comment: "why?", repoId: "planner", repoName: "orbyn-planner" }, "r")!;
    expect(chipLabel(chip)).toBe("orbyn-planner/notes.txt:2");
    expect(chipPrompt(chip)).toContain("Review on `orbyn-planner/notes.txt:2`");
    expect(chipsFromClipboard({ json: chipsClipboard([chip])!.json, html: "" })?.[0]).toMatchObject({ repoId: "planner", repoName: "orbyn-planner" });
    // The thread's own checkout has no repository, and its comment reads exactly as before.
    expect(chipLabel(asChip({ kind: "review", path: "notes.txt", line: 2, comment: "why?" })!)).toBe("notes.txt:2");
  });

  it("names a file chip's repository, kept through the clipboard", () => {
    const chip = asChip({ kind: "file", path: "notes.txt", line: 2, excerpt: "two", language: "text", repoId: "planner", repoName: "orbyn-planner" }, "f")!;
    expect(chipLabel(chip)).toBe("orbyn-planner/notes.txt:2");
    expect(chipPrompt(chip)).toContain("File `orbyn-planner/notes.txt:2`");
    expect(chipsFromClipboard({ json: chipsClipboard([chip])!.json, html: "" })?.[0]).toMatchObject({ repoId: "planner", repoName: "orbyn-planner" });
    // The thread's own checkout has no repository, and its file reads exactly as before.
    const own = asChip({ kind: "file", path: "notes.txt", line: 2 }, "o")!;
    expect(chipLabel(own)).toBe("notes.txt:2");
    expect(chipPrompt(own)).toBe("File `notes.txt:2`");
  });

  it("renders each kind as a readable prompt block", () => {
    expect(chipPrompt(chip({ kind: "file" }))).toBe("File `src/main.rs:12`\n```rust\nlet a = 1;\n```");
    expect(chipPrompt(chip({ kind: "terminal" }))).toContain("Terminal selection");
    expect(chipPrompt(chip({ kind: "review" }))).toContain("Review on `src/main.rs:12`");
    expect(chipPrompt(chip({ kind: "annotation" }))).toContain("Element `#save`");
    expect(chipPrompt(chip({ kind: "citation" }))).toBe("> Clicked save");
  });

  it("carries a picked thread as a labelled block and refuses a half-known one", () => {
    const thread = asChip({ kind: "thread", threadId: "t9", title: "Calendar feed token" }, "th")!;
    expect(chipLabel(thread)).toBe("Calendar feed token");
    expect(chipPrompt(thread)).toBe("Thread: [Calendar feed token](orbyn://thread/t9)");
    expect(promptWithChips("also check", [thread])).toBe(
      "Thread: [Calendar feed token](orbyn://thread/t9)\n\nalso check",
    );
    expect(chipsFromClipboard({ json: chipsClipboard([thread])!.json, html: "" })?.[0]).toMatchObject({
      kind: "thread", threadId: "t9", title: "Calendar feed token",
    });
    expect(asChip({ kind: "thread", threadId: "t9", title: "" })).toBeNull();
    expect(asChip({ kind: "thread", title: "No id" })).toBeNull();
  });

  it("carries a picked pull request as a labelled block and refuses a half-known one", () => {
    const pull = asChip({ kind: "pull", number: 77, title: "Tidy the docs", url: "https://github.com/o/r/pull/77" }, "pr")!;
    expect(chipLabel(pull)).toBe("#77 Tidy the docs");
    expect(chipPrompt(pull)).toBe("Pull request: [#77 Tidy the docs](https://github.com/o/r/pull/77)");
    expect(promptWithChips("see", [pull])).toBe(
      "Pull request: [#77 Tidy the docs](https://github.com/o/r/pull/77)\n\nsee",
    );
    expect(chipsFromClipboard({ json: chipsClipboard([pull])!.json, html: "" })?.[0]).toMatchObject({
      kind: "pull", number: 77, title: "Tidy the docs", url: "https://github.com/o/r/pull/77",
    });
    expect(asChip({ kind: "pull", number: 77, title: "No url" })).toBeNull();
    expect(asChip({ kind: "pull", title: "No number", url: "https://github.com/o/r/pull/77" })).toBeNull();
    expect(asChip({ kind: "pull", number: 0, title: "No number", url: "https://github.com/o/r/pull/77" })).toBeNull();
    expect(asChip({ kind: "pull", number: 77, title: "", url: "https://github.com/o/r/pull/77" })).toBeNull();
  });

  it("puts chips before the typed draft and keeps a fence inside an excerpt from closing the block", () => {
    const fenced = chip({ kind: "file", excerpt: "```js\ninner\n```" });
    expect(promptWithChips("Please fix this", [fenced])).toBe(
      "File `src/main.rs:12`\n```rust\n` ` `js\ninner\n` ` `\n```\n\nPlease fix this",
    );
    expect(promptWithChips("", [chip({ kind: "citation" })])).toBe("> Clicked save");
    const picked = chipPrompt(chip({ kind: "annotation", text: "a\n\nb", html: "<b>```</b>" }));
    expect(picked).toContain("> a b");
    expect(picked.match(/```/g)).toHaveLength(2);
  });

  it("round-trips chips through the clipboard's JSON and HTML forms", () => {
    const payload = chipsClipboard([chip({ kind: "file" })]);
    expect(payload).not.toBeNull();
    const fromJson = chipsFromClipboard({ json: payload?.json });
    expect(fromJson?.[0]).toMatchObject({ kind: "file", path: "src/main.rs", excerpt: "let a = 1;" });
    const fromHtml = chipsFromClipboard({ html: payload?.html });
    expect(fromHtml?.[0]).toMatchObject({ kind: "file", path: "src/main.rs" });
    expect(payload?.html).toContain(CHIP_ATTRIBUTE);
    expect(payload?.text).toContain("let a = 1;");
  });

  it("never turns ordinary text, broken JSON or a half-known chip into chips", () => {
    expect(chipsFromClipboard({ json: "not json" })).toBeNull();
    expect(chipsFromClipboard({ json: JSON.stringify({ version: 2, chips: [] }) })).toBeNull();
    expect(chipsFromClipboard({ html: "<p>plain</p>" })).toBeNull();
    const payload = chipsClipboard([
      { id: "x", kind: "file", path: "", line: null, excerpt: null, language: null } as ComposerChip,
    ]);
    expect(payload).toBeNull();
    expect(asChip({ kind: "file", path: "a.ts" })).toMatchObject({ kind: "file", path: "a.ts", line: null });
  });

  it("refetches a pasted file's text and keeps the chip when the read fails", async () => {
    const pasted = chip({ kind: "file", excerpt: null, language: null }) as ComposerChip;
    const filled = await refillExcerpts([pasted], async (wanted) => {
      // The read is handed the chip itself, so a paste can read another repository's file there.
      expect(wanted).toMatchObject({ kind: "file", path: "src/main.rs" });
      return { text: "fn main() {}", language: "rust" };
    });
    expect(filled[0]).toMatchObject({ excerpt: "fn main() {}", language: "rust" });
    const kept = await refillExcerpts([pasted], async () => {
      throw new Error("gone");
    });
    expect(kept[0]).toMatchObject({ path: "src/main.rs", excerpt: null });
  });

  it("hands a pasted foreign chip's repository to the read that fills it", async () => {
    const pasted = asChip({ kind: "file", path: "notes.txt", line: null, excerpt: null, repoId: "planner", repoName: "orbyn-planner" }, "f")!;
    let asked: string | undefined;
    const filled = await refillExcerpts([pasted], async (wanted) => {
      asked = wanted.repoId;
      return { text: "planner notes" };
    });
    expect(asked).toBe("planner");
    expect(filled[0]).toMatchObject({ excerpt: "planner notes", repoId: "planner", repoName: "orbyn-planner" });
  });

  it("reads the language from a path and the lines around a marked one", () => {
    expect(languageForPath("src/lib.rs")).toBe("rust");
    expect(languageForPath("README.unknown")).toBeNull();
    expect(fileExcerpt("one\ntwo\nthree\nfour", 3, 1)).toBe("two\nthree\nfour");
    expect(fileExcerpt("one\ntwo", null, 1)).toBe("one\ntwo");
  });

  it("names the terminal rows a selection covered and keeps them through a paste", () => {
    expect(chipLabel(chip({ kind: "terminal" }))).toBe("Terminal selection lines 2–3");
    expect(chipPrompt(chip({ kind: "terminal" }))).toContain("Terminal selection lines 2–3");
    expect(chipPrompt(chip({ kind: "terminal", lineStart: 5, lineEnd: 5 }))).toContain("Terminal selection line 5");
    expect(chipPrompt(chip({ kind: "terminal", lineStart: null, lineEnd: null }))).toContain("Terminal selection\n```");
    const payload = chipsClipboard([chip({ kind: "terminal" })]);
    expect(chipsFromClipboard({ json: payload?.json })?.[0]).toMatchObject({
      kind: "terminal",
      lineStart: 2,
      lineEnd: 3,
    });
    expect(asChip({ kind: "terminal", output: "ok", lineStart: 0, lineEnd: -4 })).toMatchObject({
      lineStart: null,
      lineEnd: null,
    });
  });

  it("names the marked command and its exit code, and keeps them through a paste", () => {
    const marked = chip({ kind: "terminal", command: "cargo test", exitCode: 101 });
    expect(chipLabel(marked)).toBe("Terminal selection lines 2–3 · exit 101");
    expect(chipPrompt(marked)).toBe("Terminal selection lines 2–3\nCommand: `cargo test` (exit code 101)\n```\ncargo test\nok\n```");
    expect(chipPrompt(chip({ kind: "terminal", command: "sleep 9", exitCode: null }))).toContain("Command: `sleep 9` (still running)\n");
    expect(chipPrompt(chip({ kind: "terminal", exitCode: 0 }))).toContain("lines 2–3\nExit code 0\n```");
    expect(chipPrompt(chip({ kind: "terminal", command: "echo `a`" }))).toContain("Command: `` echo `a` ``\n```");
    expect(chipPrompt(chip({ kind: "terminal", command: "for x in a\ndo echo $x; done", exitCode: 0 }))).toContain(
      "Command:\n```sh\nfor x in a\ndo echo $x; done\n```\n(exit code 0)\n```",
    );
    const payload = chipsClipboard([marked]);
    expect(chipsFromClipboard({ json: payload?.json })?.[0]).toMatchObject({ command: "cargo test", exitCode: 101 });
    // A malformed code or overlong command is dropped, not the chip.
    const kept = asChip({ kind: "terminal", output: "ok", command: "x".repeat(4097), exitCode: 1.5 });
    expect(kept).toMatchObject({ kind: "terminal", output: "ok" });
    expect(kept).not.toHaveProperty("command");
    expect(kept).not.toHaveProperty("exitCode");
  });

  it("captures a terminal selection as plain text with its buffer rows", () => {
    expect(terminalSelection("\r\n  hello\r\n世界\r\n", { start: { y: 1 }, end: { y: 2 } })).toEqual({
      output: "  hello\n世界",
      lineStart: 2,
      lineEnd: 3,
    });
    expect(terminalSelection(" \n", { start: { y: 0 }, end: { y: 0 } })).toBeNull();
    expect(terminalSelection("text", undefined)).toBeNull();
    expect(terminalSelection("text", { start: { y: 7 }, end: { y: 3 } })).toEqual({
      output: "text",
      lineStart: 4,
      lineEnd: 8,
    });
  });

  it("counts the whole message by codepoint against the service limit", () => {
    expect(MAX_PROMPT_CHARS).toBe(120_000);
    expect(charactersOver("x".repeat(MAX_PROMPT_CHARS))).toBe(0);
    expect(charactersOver("x".repeat(MAX_PROMPT_CHARS + 1))).toBe(1);
    expect(charactersOver("😀".repeat(MAX_PROMPT_CHARS + 1))).toBe(1);
    const block = promptWithChips("", [chip({ kind: "terminal" })]);
    expect(charactersOver(block + "x".repeat(MAX_PROMPT_CHARS))).toBe(block.length);
  });
});
