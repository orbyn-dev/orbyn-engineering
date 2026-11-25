import { describe, expect, it } from "vitest";
import type { ToolItem } from "../../product/timeline/types";
import { callKind, callLabel, callMs, callRuns, callState, resultRows, runLabel, runOutcomes, sourceLines, turnOutcomeText } from "./toolCalls";
import { callDuration } from "../../lib/format";

const call = (name: string, args: unknown, extra: Partial<ToolItem> = {}): ToolItem => ({
  kind: "tool",
  key: `tool:${name}:${JSON.stringify(args).length}:${Math.random()}`,
  callId: "c",
  seq: 1,
  runId: "r",
  ts: "2026-11-08T10:00:00.000Z",
  name,
  args: JSON.stringify(args),
  output: "",
  result: "",
  done: true,
  exitCode: null,
  ...extra,
});

describe("tool call rows", () => {
  it("writes durations as the rows do", () => {
    expect([100, 4200, 41_000, 62_000, 120_000].map(callDuration)).toEqual(["0.1s", "4.2s", "41s", "1m 02s", "2m 00s"]);
  });

  it("measures a call from its start to its end, or to now while it runs", () => {
    const finished = call("Bash", {}, { endedAt: "2026-11-08T10:00:04.200Z" });
    expect(callMs(finished, null)).toBe(4200);
    const running = call("Bash", {}, { result: null, done: false });
    expect(callMs(running, null)).toBeNull();
    expect(callMs(running, Date.parse("2026-11-08T10:00:14.000Z"))).toBe(14_000);
  });

  it("reads a call's state: running, failed, stopped or done", () => {
    const open = call("Bash", {}, { result: null, done: false });
    expect(callState(open, true, false)).toBe("running");
    expect(callState(open, false, true)).toBe("stopped");
    expect(callState(open, false, false)).toBe("stopped");
    expect(callState({ ...open, result: "x", done: true, exitCode: 1 }, true, false)).toBe("failed");
  });

  it("groups only the kinds that read as a group, and only when they follow each other", () => {
    const items = [
      call("Bash", { command: "a" }),
      call("Bash", { command: "b" }),
      call("Read", { file_path: "/x/one.ts" }),
      call("Grep", { pattern: "q" }),
      call("Grep", { pattern: "r" }),
      call("Read", { file_path: "/x/two.ts" }),
    ];
    expect(callRuns(items).map((run) => [run.kind, run.items.length])).toEqual([
      ["command", 2],
      ["read", 1],
      ["search", 2],
      ["read", 1],
    ]);
  });

  it("names a group by what its calls did", () => {
    const edits = callRuns([
      call("Edit", { file_path: "/x/a.ts", old_string: "one", new_string: "one\ntwo" }),
      call("Edit", { file_path: "/x/b.ts", old_string: "x\ny", new_string: "z" }),
    ])[0]!;
    expect(runLabel(edits, false)).toEqual({ verb: "Edited 2 files", note: "+3 −3" });
    const commands = callRuns([call("Bash", { command: "a" }), call("Bash", { command: "b" })])[0]!;
    expect(runLabel(commands, false).verb).toBe("Ran 2 commands");
    expect(runLabel(commands, true).verb).toBe("Running 2 commands");
  });

  it("counts a group's unmet outcomes apart from the calls its count names", () => {
    const group = (...calls: Partial<ToolItem>[]) => callRuns(calls.map((extra, at) => call("Bash", { command: `c${at}` }, extra)))[0]!;
    const ok = { exitCode: 0 };
    // One of two commands failed: the row keeps the group's count and says how many failed.
    expect(runOutcomes(group(ok, { exitCode: 1, result: "2 failed" }), false, false)).toEqual({ failed: 1, text: "1 failed" });
    expect(runOutcomes(group({ exitCode: 1 }, { exitCode: 2 }), false, false)).toEqual({ failed: 2, text: "2 failed" });
    // A call the engine never answered is not a success, and it is not silent either.
    expect(runOutcomes(group(ok, { result: null, done: false }), false, false)).toEqual({ failed: 0, text: "1 no result reported" });
    // A finished call that reported no outcome is unknown, never counted as one that worked.
    expect(runOutcomes(group(ok, { exitCode: null, done: true, result: "ok" }), false, false)).toEqual({ failed: 0, text: "1 outcome unknown" });
    // While the calls are still running, nothing is missing yet, and a group of successes says nothing.
    expect(runOutcomes(group(ok, { result: null, done: false }), true, false)).toBeNull();
    expect(runOutcomes(group(ok, ok), false, false)).toBeNull();
  });

  it("reads a resultless call as unknown when its run was interrupted, and as no result when the run ended normally", () => {
    const group = (...calls: Partial<ToolItem>[]) => callRuns(calls.map((extra, at) => call("Bash", { command: `c${at}` }, extra)))[0]!;
    const open = { exitCode: null, result: null, done: false };
    expect(runOutcomes(group({ exitCode: 0 }, open), false, true)).toEqual({ failed: 0, text: "1 outcome unknown" });
    expect(runOutcomes(group(open, open), false, true)).toEqual({ failed: 0, text: "2 outcomes unknown" });
    expect(runOutcomes(group({ exitCode: 0 }, open), false, false)).toEqual({ failed: 0, text: "1 no result reported" });
    expect(runOutcomes(group(open, open), false, false)).toEqual({ failed: 0, text: "2 no results reported" });
  });

  it("states only the most serious outcome of a mixed group", () => {
    const group = (...calls: Partial<ToolItem>[]) => callRuns(calls.map((extra, at) => call("Bash", { command: `c${at}` }, extra)))[0]!;
    const mixed = group({ exitCode: 0 }, { exitCode: 1 }, { result: null, done: false }, { exitCode: null, result: "ok" });
    expect(runOutcomes(mixed, false, false)).toEqual({ failed: 1, text: "1 failed" });
    expect(runOutcomes(mixed, false, true)).toEqual({ failed: 1, text: "1 failed" });
  });

  it("counts a finished turn's calls by the four outcomes the engine reported", () => {
    const turn = [
      call("Bash", { command: "npm test" }, { exitCode: 0 }),
      call("Bash", { command: "npm run lint" }, { exitCode: 1, result: "1 failed" }),
      call("Bash", { command: "git fetch" }, { result: null, done: false }),
      call("Bash", { command: "npm run build" }, { exitCode: null, result: "built" }),
    ];
    expect(turnOutcomeText(turn, false, false)).toBe("4 tool calls · 1 succeeded · 1 failed · 1 no result reported · 1 outcome unknown");
  });

  it("reads a turn's resultless call as unknown when the run was interrupted, not no result reported", () => {
    const turn = [call("Bash", { command: "ls" }, { exitCode: 0 }), call("Bash", { command: "git fetch" }, { result: null, done: false })];
    expect(turnOutcomeText(turn, false, false)).toBe("2 tool calls · 1 succeeded · 1 no result reported");
    expect(turnOutcomeText(turn, false, true)).toBe("2 tool calls · 1 succeeded · 1 outcome unknown");
  });

  it("lists only the outcomes a turn has, and keeps a call still running out of them", () => {
    const done = [0, 1, 2, 3, 4, 5, 6, 7].map((at) => call("Bash", { command: `ok${at}` }, { exitCode: 0 }));
    expect(turnOutcomeText(done, false, false)).toBe("8 tool calls · 8 succeeded");
    const live = [...done, call("Bash", { command: "sync" }, { result: null, done: false })];
    expect(turnOutcomeText(live, true, false)).toBe("9 tool calls · 8 succeeded");
    expect(turnOutcomeText(live, false, false)).toBe("9 tool calls · 8 succeeded · 1 no result reported");
  });

  it("counts many missing calls in their own words", () => {
    const turn = [
      call("Bash", { command: "a" }, { result: null, done: false }),
      call("Bash", { command: "b" }, { result: null, done: false }),
      call("Bash", { command: "c" }, { exitCode: null, result: "ok" }),
      call("Bash", { command: "d" }, { exitCode: null, result: "ok" }),
    ];
    expect(turnOutcomeText(turn, false, false)).toBe("4 tool calls · 2 no results reported · 2 outcomes unknown");
  });

  it("labels each kind with its own verb, object and note", () => {
    const label = (item: ToolItem) => {
      const { verb, object, note } = callLabel(item, "/work");
      return [verb, object, note];
    };
    expect(label(call("Read", { file_path: "/work/auth/redirect.ts", offset: 12, limit: 18 }))).toEqual(["Read", "auth/redirect.ts", "· lines 12–29"]);
    expect(label(call("Grep", { pattern: "AGENTS.md" }, { result: "a.md:1:x\nb.md:2:y" }))).toEqual(["Searched for “AGENTS.md”", null, "· 2 results"]);
    expect(label(call("mcp__orbyn-memory__find", { query: "login redirect", limit: 5 }))).toEqual(["orbyn-memory", "find", "· “login redirect”"]);
    expect(label(call("WebFetch", { url: "https://cheatsheetseries.owasp.org/x.html" }))).toEqual(["Fetched", "cheatsheetseries.owasp.org", null]);
    expect(callKind(call("Write", { file_path: "/x", content: "a" }))).toBe("create");
  });

  it("reads search output as path:line rows and a read as numbered lines", () => {
    expect(resultRows("Found 2 files\nsrc/a.ts:3:const x\nsrc/b.ts")).toEqual([
      { where: "src/a.ts:3", line: 3, text: "const x" },
      { where: "src/b.ts", line: null, text: "" },
    ]);
    expect(resultRows("No matches found")).toEqual([]);
    expect(sourceLines("    12→export function a() {\n    13→}", 1)).toEqual([
      { n: 12, text: "export function a() {" },
      { n: 13, text: "}" },
    ]);
    expect(sourceLines("a\nb", 5).map((line) => line.n)).toEqual([5, 6]);
  });
});
