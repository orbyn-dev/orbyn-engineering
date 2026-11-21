import { describe, expect, it } from "vitest";
import type { ComposerChip, ReviewChip } from "./composerContext";
import { batchSummary, groupByFile, reviewDrafts, reviewMessage } from "./reviewBatch";

const draft = (id: string, path: string, line: number, comment: string, over: Partial<ReviewChip> = {}): ReviewChip =>
  ({ id, kind: "review", path, line, comment, excerpt: `src ${id}`, ...over });

describe("review batch", () => {
  it("keeps only review comments, in order", () => {
    const other: ComposerChip = { id: "f", kind: "file", path: "x.ts", line: null, excerpt: null, language: null };
    expect(reviewDrafts([draft("a", "a.ts", 1, "one"), other, draft("b", "b.ts", 2, "two")]).map((d) => d.id)).toEqual(["a", "b"]);
    expect(reviewDrafts(undefined)).toEqual([]);
  });

  it("counts comments and distinct files", () => {
    expect(batchSummary([draft("a", "a.ts", 1, "x"), draft("b", "a.ts", 9, "y"), draft("c", "b.ts", 2, "z")])).toEqual({ comments: 3, files: 2 });
  });

  it("groups by file in first-written order", () => {
    const groups = groupByFile([draft("a", "b.ts", 1, "x"), draft("b", "a.ts", 1, "y"), draft("c", "b.ts", 4, "z")]);
    expect(groups.map((g) => [g.path, g.drafts.map((d) => d.id)])).toEqual([["b.ts", ["a", "c"]], ["a.ts", ["b"]]]);
  });

  it("builds one message with a count heading and every comment in written order", () => {
    const message = reviewMessage([draft("a", "z.ts", 15, "first"), draft("b", "a.ts", 8, "second", { stale: true })], "fix/login");
    expect(message.startsWith("Review comments on fix/login (2)\n\n")).toBe(true);
    expect(message.indexOf("z.ts:15")).toBeLessThan(message.indexOf("a.ts:8"));
    expect(message).toContain("first");
    expect(message).toContain("src a");
    expect(message).toContain("this text may have changed or moved");
  });

  it("leaves the branch out when the thread has none", () => {
    expect(reviewMessage([draft("a", "a.ts", 1, "x")], null).split("\n")[0]).toBe("Review comments (1)");
  });
});
