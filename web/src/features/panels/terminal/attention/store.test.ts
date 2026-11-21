import { beforeEach, describe, expect, it } from "vitest";
import { applyAttention, emptyAttention, parseAttention, useAttention, type AttentionState } from "./store";

const waiting = (over: Record<string, unknown> = {}) => ({
  v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "yes_no",
  since: "2026-11-22T14:12:00Z", excerpt: ["Overwrite auth/redirect.ts? (y/N)"], ...over,
});
const at = (seq: number, value: unknown) => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: "t1", event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });

describe("parseAttention", () => {
  it("accepts a waiting event and a clear event", () => {
    expect(parseAttention(waiting())?.op).toBe("waiting");
    expect(parseAttention({ v: 1, op: "clear", id: "a1", reason: "input" })?.op).toBe("clear");
  });
  it.each([
    ["an unknown version", waiting({ v: 2 })],
    ["an unknown kind", waiting({ kind: "panic" })],
    ["a missing id", waiting({ id: "" })],
    ["a long id", waiting({ id: "x".repeat(65) })],
    ["an excerpt that is not lines", waiting({ excerpt: "text" })],
    ["a bad time", waiting({ since: "yesterday" })],
    ["a clear with an unknown reason", { v: 1, op: "clear", id: "a1", reason: "x" }],
    ["null", null],
    ["a string", "waiting"],
  ])("refuses %s", (_name, value) => {
    expect(parseAttention(value)).toBeNull();
  });
  it("caps and cleans the excerpt, and keeps only the label for a secret", () => {
    const long = parseAttention(waiting({ excerpt: Array.from({ length: 20 }, (_, i) => `\u001b[31mline ${i}\u001b[0m${"x".repeat(300)}`) }));
    if (long?.op !== "waiting") throw new Error("expected waiting");
    expect(long.excerpt.length).toBeLessThanOrEqual(6);
    expect(long.excerpt.join("")).not.toContain("\u001b");
    expect(long.excerpt.every((line) => line.length <= 200)).toBe(true);
    expect(long.excerpt.join("\n").length).toBeLessThanOrEqual(1000);
    const secret = parseAttention(waiting({ kind: "secret", excerpt: ["Password:", "hunter2", "more"] }));
    if (secret?.op !== "waiting") throw new Error("expected waiting");
    expect(secret.excerpt).toEqual(["Password:"]);
  });
  it("keeps markup as text", () => {
    const event = parseAttention(waiting({ excerpt: ["<img src=x onerror=alert(1)>"], terminal_title: "<b>dev</b>" }));
    if (event?.op !== "waiting") throw new Error("expected waiting");
    expect(event.excerpt[0]).toBe("<img src=x onerror=alert(1)>");
  });
});

describe("applyAttention", () => {
  let state: AttentionState;
  beforeEach(() => { state = emptyAttention(); });

  it("adds an item once and says it is fresh only the first time", () => {
    const first = applyAttention(state, at(1, waiting()));
    expect(first.fresh?.id).toBe("a1");
    const again = applyAttention(first.state, at(2, waiting({ excerpt: ["Overwrite? (y/N)"] })));
    expect(again.fresh).toBeNull();
    expect(again.state.entries.get("a1")?.excerpt).toEqual(["Overwrite? (y/N)"]);
  });
  it("latest wins: an older event for an id changes nothing", () => {
    const newer = applyAttention(state, at(5, waiting({ excerpt: ["new"] }))).state;
    const older = applyAttention(newer, at(3, waiting({ excerpt: ["old"] }))).state;
    expect(older.entries.get("a1")?.excerpt).toEqual(["new"]);
  });
  it("a clear marks the item cleared, and a late waiting does not bring it back", () => {
    const one = applyAttention(state, at(1, waiting())).state;
    const cleared = applyAttention(one, at(4, { v: 1, op: "clear", id: "a1", reason: "input" })).state;
    expect(cleared.entries.get("a1")?.cleared?.reason).toBe("input");
    const late = applyAttention(cleared, at(2, waiting()));
    expect(late.fresh).toBeNull();
    expect(late.state.entries.get("a1")?.cleared).not.toBeNull();
  });
  it("a clear that arrives first stops the older waiting", () => {
    const early = applyAttention(state, at(9, { v: 1, op: "clear", id: "a1", reason: "closed" })).state;
    const late = applyAttention(early, at(7, waiting()));
    expect(late.state.entries.has("a1")).toBe(false);
    expect(late.fresh).toBeNull();
  });
  it("ignores another event name", () => {
    const other = applyAttention(state, { seq: 1, ts: "2026-11-22T14:12:03Z", thread_id: "t1", event: { type: "CUSTOM", name: "other", value: waiting() } });
    expect(other.state.entries.size).toBe(0);
  });
  it("uses a Map: an id like __proto__ is just a key", () => {
    const next = applyAttention(state, at(1, waiting({ id: "__proto__" }))).state;
    expect(next.entries.get("__proto__")?.id).toBe("__proto__");
  });
  it("keeps at most 64 entries and keeps the newest", () => {
    let next = state;
    for (let i = 0; i < 70; i += 1) next = applyAttention(next, at(i + 1, waiting({ id: `a${i}` }))).state;
    expect(next.entries.size).toBeLessThanOrEqual(64);
    expect(next.entries.has("a69")).toBe(true);
  });
});

describe("the store", () => {
  beforeEach(() => useAttention.getState().reset());
  it("dismiss hides one item and survives an update of it", () => {
    useAttention.getState().receive(at(1, waiting()));
    useAttention.getState().dismiss("a1");
    useAttention.getState().receive(at(2, waiting({ excerpt: ["again"] })));
    expect(useAttention.getState().entries.get("a1")?.dismissed).toBe(true);
  });
});
