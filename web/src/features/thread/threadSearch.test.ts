import { describe, expect, it } from "vitest";
import type { MessageItem, ToolItem } from "../../product/timeline/types";
import { findMatches, rowOfItems, rowWhere } from "./threadSearch";
import type { TimelineEntry } from "../../product/timeline/timeline";

const message = (key: string, text: string, role = "assistant"): MessageItem => ({
  kind: "message", key, seq: 0, ts: "", runId: "r", role, text, done: true,
});
const tool = (key: string, name: string, args: string): ToolItem => ({
  kind: "tool", key, seq: 0, ts: "", runId: "r", callId: key, name, args, output: "", result: null, done: true, exitCode: 0,
});

const entries: TimelineEntry[] = [
  message("u1", "find the Needle please", "user"),
  { kind: "fold", key: "fold:r", label: "Ran 1 command", entries: [
    { kind: "work", key: "work:t1", items: [tool("t1", "shell", "grep needle src")] },
    message("a1", "A needle in the fold, and another NEEDLE"),
  ] },
  message("a2", "Nothing here"),
];

describe("thread search, shaped like what is shown", () => {
  it("counts rendered Markdown text, not link targets, fences or markers", () => {
    const source = "see [the needle docs](https://example.com/needle) and **bold needle**\n\n```ts\nconst needle = 1\n```\n\nplain https://example.com/needle link";
    const rows: TimelineEntry[] = [message("a", source)];
    // The label, the bold word and the code line; the two URLs are shown only as the bare link.
    expect(findMatches(rows, "needle")).toHaveLength(4);
    expect(findMatches(rows, "https://example.com/needle")).toHaveLength(1);
    expect(findMatches(rows, "```")).toHaveLength(0);
    expect(findMatches(rows, "**")).toHaveLength(0);
  });

  it("finds text in a tool card's output, not only its command", () => {
    const rows: TimelineEntry[] = [{ kind: "work", key: "w", items: [{ ...tool("t", "shell", "npm test"), output: "FAIL zq-output-marker\n", result: null }] }];
    expect(findMatches(rows, "zq-output-marker")).toEqual([{ index: 0, nth: 0 }]);
    expect(findMatches(rows, "npm test")).toHaveLength(1);
  });
});

describe("thread search counts what a tool card shows", () => {
  const finished = (args: string, output: string, result: string | null): TimelineEntry => ({
    kind: "work",
    key: "w",
    items: [{ ...tool("t", "shell", args), output, result, done: true }],
  });

  it("counts a finished command's text once when the streamed output and the result are the same", () => {
    const rows = [finished(JSON.stringify({ command: "echo hi" }), "FAIL zq-marker\n", "FAIL zq-marker\n")];
    expect(findMatches(rows, "zq-marker")).toHaveLength(1);
  });

  it("counts the output while it streams, before there is a result", () => {
    expect(findMatches([finished("ls", "zq-marker\n", null)], "zq-marker")).toHaveLength(1);
  });

  it("does not count the JSON keys of a command's arguments, which the card replaces with the command", () => {
    const rows = [finished(JSON.stringify({ command: "echo hi" }), "ok", "ok")];
    expect(findMatches(rows, "command")).toHaveLength(0);
    expect(findMatches(rows, "echo hi")).toHaveLength(1);
  });

  it("counts the pretty JSON the card shows for a tool without a command", () => {
    const rows = [finished(JSON.stringify({ path: "a.txt", mode: "read" }), "ok", "ok")];
    expect(findMatches(rows, '"mode"')).toHaveLength(1);
    expect(findMatches(rows, "a.txt")).toHaveLength(1);
  });

  it("counts a command's own text, and output without terminal escape codes", () => {
    const rows = [finished("ls -la", "\u001b[32mgreen word\u001b[0m\n", null)];
    expect(findMatches(rows, "ls -la")).toHaveLength(1);
    expect(findMatches(rows, "green word")).toHaveLength(1);
    expect(findMatches(rows, "[32m")).toHaveLength(0);
  });

  it("does not count a diff summary's file paths, which are button labels behind a toggle", () => {
    const diff: TimelineEntry = { kind: "diff", key: "d", seq: 0, ts: "", runId: "r", files: [{ path: "src/zq-file.ts", additions: 1, deletions: 0 }] };
    expect(findMatches([diff], "zq-file")).toHaveLength(0);
  });
});

describe("thread search", () => {
  it("counts every occurrence in order, case-insensitively, including inside folds", () => {
    expect(findMatches(entries, "needle")).toEqual([
      { index: 0, nth: 0 },
      { index: 1, nth: 0 },
      { index: 1, nth: 1 },
      { index: 1, nth: 2 },
    ]);
  });

  it("finds nothing for empty or absent text", () => {
    expect(findMatches(entries, "")).toEqual([]);
    expect(findMatches(entries, "walrus")).toEqual([]);
  });

  it("maps every item, folded or not, to the row that holds it", () => {
    const rows = rowOfItems(entries);
    expect(rows.get("u1")).toBe(0);
    expect(rows.get("a1")).toBe(1);
    expect(rows.get("t1")).toBe(1);
    expect(rows.get("a2")).toBe(2);
  });

  it("locates the first row holding an item that matches a test", () => {
    expect(rowWhere(entries, (item) => item.kind === "tool")).toBe(1);
    expect(rowWhere(entries, (item) => item.kind === "approval")).toBe(-1);
  });
});
