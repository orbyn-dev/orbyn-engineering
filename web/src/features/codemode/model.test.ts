import { describe, expect, it } from "vitest";
import {
  boundedLines,
  codeToolOf,
  normalizeCall,
  readReply,
  rowSummary,
  runView,
  scriptMeta,
  shortHash,
  viaOf,
} from "./model";
import type { CodeRunRecord } from "./types";

const record = (extra: Partial<CodeRunRecord> = {}): CodeRunRecord => ({
  run_id: "run_7c41e2",
  status: "completed",
  script: "const a = 1;\nreturn a;\n",
  started_at: "2026-11-21T10:00:00.000Z",
  ended_at: "2026-11-21T10:00:03.100Z",
  calls: [],
  ...extra,
});

describe("which Code Mode tool a call is", () => {
  it("reads the tool out of the engine's MCP name and ignores everything else", () => {
    expect(codeToolOf("mcp__orbyn_engineering__orbyn_code_run")).toBe("run");
    expect(codeToolOf("mcp.orbyn_engineering.orbyn_code_search")).toBe("search");
    expect(codeToolOf("mcp__orbyn_engineering__orbyn_code_describe")).toBe("describe");
    expect(codeToolOf("mcp__orbyn_engineering__orbyn_thread_read")).toBeNull();
    // Another server with a tool of the same name is not Orbyn's.
    expect(codeToolOf("mcp__other__orbyn_code_run")).toBeNull();
    expect(codeToolOf("Bash")).toBeNull();
  });
});

describe("script size", () => {
  it("counts lines and bytes the way the frames say them", () => {
    expect(scriptMeta("a\nb\nc\n")).toMatchObject({ lines: 3, bytes: 6, header: "Script · 3 lines · 6 B" });
    expect(scriptMeta("x").header).toBe("Script · 1 line · 1 B");
    expect(scriptMeta("é").bytes).toBe(2);
    expect(scriptMeta("x\n".repeat(412)).header).toBe("Script · 412 lines · 824 B");
    expect(scriptMeta("y".repeat(38 * 1024)).header).toBe("Script · 1 line · 38 KiB");
    expect(scriptMeta("").lines).toBe(0);
  });

  it("shortens a hash to its head and tail", () => {
    expect(shortHash("3f9a1c88aa00bb11cc22dd33ee44ff5566778899aabbccddeeff00112233e07")).toBe("3f9a1c…e07");
    expect(shortHash("abc")).toBe("abc");
  });
});

describe("bounded text", () => {
  it("keeps the newest lines and says how many were cut", () => {
    const lines = Array.from({ length: 700 }, (_, at) => `line ${at}`);
    const cut = boundedLines(lines, 500);
    expect(cut.shown).toHaveLength(500);
    expect(cut.shown[499]).toBe("line 699");
    expect(cut.hidden).toBe(200);
  });

  it("cuts a very long line", () => {
    expect(boundedLines(["x".repeat(5000)], 10, 100).shown[0]).toHaveLength(101);
  });
});

describe("the approval's via field", () => {
  const base = { id: "a", thread_id: "t", run_id: "r", kind: "tool", summary: "Use orbyn_thread_send", detail: {}, state: "pending", decided_by: null, decided_at: null, created_at: "x" };
  it("reads a well formed via", () => {
    expect(viaOf({ ...base, via: { code_run: "run_1", call: 5, script_sha256: "ab" } } as never)).toEqual({ codeRun: "run_1", call: 5, scriptSha256: "ab" });
  });
  it("is null when absent or malformed, so an ordinary approval is never touched", () => {
    expect(viaOf(base as never)).toBeNull();
    expect(viaOf({ ...base, via: { code_run: 4 } } as never)).toBeNull();
    expect(viaOf({ ...base, via: { code_run: "run_1", call: "5" } } as never)).toBeNull();
    expect(viaOf({ ...base, via: "run_1" } as never)).toBeNull();
  });
});

describe("a call's status", () => {
  it("maps the words the service may use, and never reads an unknown word as a success", () => {
    expect(normalizeCall({ index: 1, tool: "t", status: "ok" }).status).toBe("done");
    expect(normalizeCall({ index: 1, tool: "t", status: "denied" }).status).toBe("declined");
    expect(normalizeCall({ index: 1, tool: "t", status: "timed_out" }).status).toBe("expired");
    expect(normalizeCall({ index: 1, tool: "t", status: "invalid" }).status).toBe("refused");
    expect(normalizeCall({ index: 1, tool: "t", status: "held" }).status).toBe("waiting");
    expect(normalizeCall({ index: 1, tool: "t", status: "in_flight" }).status).toBe("interrupted");
    expect(normalizeCall({ index: 1, tool: "t", status: "teleported" }).status).toBe("unknown");
  });
});

describe("the reply the engine read", () => {
  it("parses a completed, paused and error reply", () => {
    expect(readReply('{"status":"completed","result":["t_52"],"logs":["a"],"runId":"run_1"}')).toMatchObject({ status: "completed", result: ["t_52"], logs: ["a"], runId: "run_1" });
    expect(readReply('{"status":"paused","runId":"run_1","pending":[{"tool":"orbyn_thread_send","call":5}]}')).toMatchObject({ status: "paused", pending: [{ tool: "orbyn_thread_send", call: 5 }] });
    expect(readReply('{"status":"error","runId":"r","error":{"kind":"syntax","message":"Unexpected token","line":7}}')?.error).toEqual({ kind: "syntax", message: "Unexpected token", line: 7 });
  });
  it("is null for text that is not a reply, and never throws", () => {
    expect(readReply("hello")).toBeNull();
    expect(readReply(null)).toBeNull();
    expect(readReply('{"status":"sideways"}')).toBeNull();
    expect(readReply('{"status":"error","error":{"kind":"mystery","message":"x"}}')?.error?.kind).toBe("runtime");
  });
});

describe("the run the block shows", () => {
  it("prefers the run record and falls back to the reply", () => {
    const fromRecord = runView(record({ calls: [{ index: 1, tool: "orbyn_thread_list", status: "ok", duration_ms: 100 }] }), null);
    expect(fromRecord).toMatchObject({ status: "completed", calls: [{ tool: "orbyn_thread_list", status: "done" }], hasRecord: true });
    const fromReply = runView(null, { status: "error", runId: "run_9", error: { kind: "limit", message: "A script may make 200 calls." }, logs: ["x"] });
    expect(fromReply).toMatchObject({ status: "error", runId: "run_9", hasRecord: false, calls: [], error: { kind: "limit" } });
  });

  it("is null when there is neither", () => {
    expect(runView(null, null)).toBeNull();
  });
});

describe("the row", () => {
  const now = Date.parse("2026-11-21T10:00:06.000Z");
  it("says what the frames say for each state", () => {
    const calls = (n: number) => Array.from({ length: n }, (_, at) => ({ index: at + 1, tool: "t", status: "ok" }));
    const running = runView(record({ status: "running", ended_at: null, calls: calls(4) }), null)!;
    expect(rowSummary(running, now)).toEqual({ note: "· 4 calls so far", due: "6s", tone: "run" });
    const done = runView(record({ calls: calls(6) }), null)!;
    expect(rowSummary(done, now)).toEqual({ note: "· 6 calls", due: "3.1s", tone: "done" });
    const paused = runView(record({ status: "paused", ended_at: null, calls: calls(4), pending: [{ tool: "orbyn_thread_send", call: 5 }] }), null)!;
    expect(rowSummary(paused, now)).toEqual({ note: "· 4 calls done", due: "Paused", tone: "wait" });
    const syntax = runView(record({ status: "error", error: { kind: "syntax", message: "x", line: 7 } }), null)!;
    expect(rowSummary(syntax, now)).toEqual({ note: "· syntax error", due: "Failed", tone: "fail" });
    expect(rowSummary(runView(record({ status: "error", error: { kind: "runtime", message: "x" } }), null)!, now).note).toBe("· runtime error");
    expect(rowSummary(runView(record({ status: "error", error: { kind: "limit", message: "x" } }), null)!, now).note).toBe("· limit");
    expect(rowSummary(runView(record({ status: "error", error: { kind: "denied", message: "x" } }), null)!, now).note).toBe("· denied");
    expect(rowSummary(runView(record({ status: "error", error: { kind: "lost", message: "x" } }), null)!, now)).toEqual({ note: "· core restarted", due: "Lost", tone: "fail" });
    const resumed = runView(record({ status: "running", ended_at: null, resumes: 1, calls: calls(6) }), null)!;
    expect(rowSummary(resumed, now).note).toBe("· resumed");
    expect(rowSummary(runView(record({ resumes: 1, calls: calls(6), ended_at: "2026-11-21T10:01:04.000Z" }), null)!, now).due).toBe("1m 04s");
  });

  it("counts only finished calls as done on a paused run", () => {
    const v = runView(record({ status: "paused", ended_at: null, calls: [{ index: 1, tool: "t", status: "ok" }, { index: 2, tool: "t", status: "waiting" }] }), null)!;
    expect(rowSummary(v, now).note).toBe("· 1 call done");
  });

  it("counts one call as a call", () => {
    const v = runView(record({ calls: [{ index: 1, tool: "t", status: "ok" }] }), null)!;
    expect(rowSummary(v, now).note).toBe("· 1 call");
  });
});
