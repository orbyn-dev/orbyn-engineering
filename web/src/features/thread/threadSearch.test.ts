import { describe, expect, it } from "vitest";
import type { MessageItem, ToolItem } from "./projection";
import { findMatches, rowOfItems, rowWhere } from "./threadSearch";
import type { TimelineEntry } from "./timeline";

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
