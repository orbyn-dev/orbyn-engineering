import { describe, expect, it } from "vitest";
import type { Thread } from "../../api/types";
import type { ThreadFamily } from "../../generated/ThreadFamily";
import { familyRows, otherCount, parentLine, relationLabel, rowStatus } from "./model";

const NOW = Date.parse("2026-11-21T12:00:00Z");
const make = (id: string, parent: string | null, relation: string | null, over: Partial<Thread> = {}): Thread =>
  ({
    id,
    project_id: "p1",
    title: `Thread ${id}`,
    engine: "claude",
    model: null,
    cwd: "/x",
    worktree: null,
    skill_profile: null,
    active_run: null,
    last_run_state: "completed",
    created_at: "2026-11-20T12:00:00Z",
    updated_at: "2026-11-21T10:00:00Z",
    scratch: false,
    lifecycle: { parent_thread_id: parent, relation } as Thread["lifecycle"],
    ...over,
  }) as Thread;

const family = (threads: Thread[], thread_id = "a"): ThreadFamily => ({ thread_id, root_thread_id: "a", threads });

describe("familyRows", () => {
  const tree = family([make("a", null, null), make("b", "a", "fork"), make("c", "b", "subagent"), make("d", "a", "side")], "c");

  it("keeps the service order and gives each thread its depth", () => {
    expect(familyRows(tree, "c").map((row) => [row.thread.id, row.depth])).toEqual([["a", 0], ["b", 1], ["c", 2], ["d", 1]]);
  });

  it("marks the root and the open thread", () => {
    const rows = familyRows(tree, "c");
    expect(rows.map((row) => row.root)).toEqual([true, false, false, false]);
    expect(rows.map((row) => row.current)).toEqual([false, false, true, false]);
  });

  it("names the relation, and nothing for the root", () => {
    expect(familyRows(tree, "c").map((row) => row.relation)).toEqual([null, "fork", "subagent", "side"]);
  });

  it("treats a member whose parent is missing as a top-level row", () => {
    const rows = familyRows(family([make("a", null, null), make("x", "gone", "fork")]), "a");
    expect(rows[1]).toMatchObject({ depth: 0, relation: "fork" });
  });

  it("does not loop on a cycle", () => {
    const rows = familyRows(family([make("a", "b", "fork"), make("b", "a", "fork")]), "a");
    expect(rows).toHaveLength(2);
  });
});

describe("otherCount", () => {
  it("counts the other threads in the family", () => {
    expect(otherCount(family([make("a", null, null), make("b", "a", "fork")]))).toBe(1);
    expect(otherCount(family([make("a", null, null)]))).toBe(0);
  });
});

describe("relationLabel", () => {
  it("names fork, side and subagent, and refuses an unknown word", () => {
    expect(relationLabel("fork")).toBe("Fork");
    expect(relationLabel("side")).toBe("Side chat");
    expect(relationLabel("subagent")).toBe("Subagent");
    expect(relationLabel("hax")).toBeNull();
    expect(relationLabel(null)).toBeNull();
  });
});

describe("parentLine", () => {
  const threads = { a: make("a", null, null), b: make("b", "a", "side") };
  it("is the relation and the parent's title", () => {
    expect(parentLine(threads.b, threads)).toEqual({ relation: "Side chat", parent: threads.a });
  });
  it("is nothing for a thread with no parent, or whose parent is not known", () => {
    expect(parentLine(threads.a, threads)).toBeNull();
    expect(parentLine(make("z", "gone", "fork"), threads)).toBeNull();
  });
  it("is nothing for a relation the app does not know", () => {
    expect(parentLine(make("z", "a", "other"), threads)).toBeNull();
  });
});

describe("rowStatus", () => {
  it("says running, failed, or how long ago", () => {
    expect(rowStatus(make("a", null, null, { active_run: { state: "running" } as Thread["active_run"] }), NOW)).toEqual({ kind: "running", label: "Running" });
    expect(rowStatus(make("a", null, null, { last_run_state: "failed" }), NOW)).toEqual({ kind: "failed", label: "Failed" });
    expect(rowStatus(make("a", null, null), NOW)).toEqual({ kind: "age", label: "2h" });
  });
  it("says Waiting for you for an approval", () => {
    expect(rowStatus(make("a", null, null, { active_run: { state: "awaiting_approval" } as Thread["active_run"] }), NOW)).toEqual({ kind: "waiting", label: "Waiting for you" });
  });
});
