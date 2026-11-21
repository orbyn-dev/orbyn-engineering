import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Connection } from "@muse-code/sdk";
import { AsyncQueue } from "../src/asyncQueue.mjs";

// Replays a T3 Code muse_* transcript (MIT, see test/fixtures/NOTICE) under the real SDK Connection.
// The recorded wire is JSON-RPC over the stdio of `muse serve`: expect_outbound entries are checked
// against the frames the family writes, emit_inbound entries are fed to the Connection as the server's
// lines, and the pump blocks at the next expected frame. The Connection mints the recorded command ids
// (metadata.commandIds), so request ids and ids line up one for one. `<workspace>` is replaced.
// turn/start is compared on the fields the family owns: T3 wraps the prompt in its own runtime_info.
const TURN_START_KEYS = ["sessionId", "commandId", "workspaceRoots", "ifBusy"];

const sub = (value, workspace) => JSON.parse(JSON.stringify(value).replaceAll('"<workspace>"', JSON.stringify(workspace)));

export function replayMuse(source, { workspace = "/work" } = {}) {
  const entries = (Array.isArray(source) ? source : readFileSync(source, "utf8").trim().split("\n").map((l) => JSON.parse(l)));
  assert.equal(entries[0].type, "transcript_start");
  const ids = (entries[0].metadata?.commandIds ?? []).flat();
  const hostStart = entries[1];
  assert.equal(hostStart.label, "host_start");
  const body = entries.slice(2);
  const spawns = [];
  const failures = [];
  const outbound = [];
  const incoming = new AsyncQueue();
  let cursor = 0;
  let broken = false;

  const pump = () => {
    while (cursor < body.length && body[cursor].type === "emit_inbound") {
      incoming.push(`${JSON.stringify(sub(body[cursor].frame, workspace))}\n`);
      cursor++;
    }
  };

  const check = (expected, actual) => {
    assert.equal(actual.method, expected.method, "method");
    if (expected.id !== undefined) assert.equal(actual.id, expected.id, `${expected.method} id`);
    if (expected.result !== undefined) assert.deepEqual(actual.result, expected.result, "reply result");
    if (expected.params === undefined || expected.method === "initialize") return; // clientInfo and requested capabilities are ours
    if (expected.method === "turn/start") {
      for (const key of TURN_START_KEYS) assert.deepEqual(actual.params[key], expected.params[key], `turn/start.${key}`);
      assert.deepEqual(actual.params.input.at(-1), expected.params.input.at(-1), "turn/start prompt text");
      return;
    }
    let wanted = expected.params;
    // T3 recorded two sessions full-access (allowAll, :unrestricted). The host never grants that: the core
    // decides, so every session asks (promptUnmatched, :ask-me) and only the arguments below follow the recording.
    if (expected.method === "session/start" && wanted.approvalMode === "allowAll") wanted = { ...wanted, approvalMode: "promptUnmatched", permissionProfile: ":ask-me" };
    assert.deepEqual(actual.params, wanted, `${expected.method} params`);
  };

  const onOutbound = (frame) => {
    outbound.push(frame);
    const entry = body[cursor];
    if (broken) return;
    try {
      assert.ok(entry && entry.type === "expect_outbound", `unexpected outbound ${frame.method ?? "reply"}; the transcript has ${entry ? entry.type : "ended"}`);
      check(sub(entry.frame, workspace), frame);
    } catch (cause) {
      broken = true;
      failures.push(`${entry?.label}: ${cause.message}`);
      throw cause;
    }
    cursor++;
    pump();
  };

  const transport = {
    incoming,
    async write(chunk) {
      for (const line of chunk.split("\n")) if (line.trim()) onOutbound(JSON.parse(line));
    },
    close() {
      incoming.close();
    },
  };

  let connection;
  let exitOk = () => {};
  let closedFlag = false;
  const spawn = (options) => {
    spawns.push(options);
    const wanted = hostStart.frame.args;
    assert.deepEqual(options.args.slice(options.args.indexOf("serve")), wanted, "muse serve arguments");
    const exited = new Promise((resolve) => (exitOk = resolve));
    const handshake = {
      child: { stderrTail: [] },
      exited,
      onServerRequest() {},
      async initialize(params) {
        const queue = [...ids];
        connection = new Connection(transport, { mintCommandId: () => queue.shift() ?? crypto.randomUUID() });
        pump();
        const initializeResult = await connection.request("initialize", params);
        connection.notify("initialized");
        await connection.flush();
        return { connection, child: handshake.child, initializeResult, exited, close: handshake.close };
      },
      async close() {
        closedFlag = true;
        transport.close();
        exitOk({ code: 0, signal: null });
        return { code: 0, signal: null };
      },
    };
    handshake.close = handshake.close.bind(handshake);
    return handshake;
  };

  return {
    spawn,
    spawns,
    failures,
    outbound,
    /** Expected frames the family never wrote (a complete test leaves none). */
    remaining: () => body.slice(cursor).filter((e) => e.type === "expect_outbound").map((e) => e.label),
    /** Methods of the frames the family wrote, in order. */
    methods: () => outbound.map((f) => f.method ?? "reply"),
    /** The prompt text recorded in the nth turn/start. */
    promptTexts: () => body.filter((e) => e.label === "turn/start").map((e) => e.frame.params.input.at(-1).text),
    sessionId: () => body.find((e) => e.label === "session/start").frame.params.sessionId,
    /** The `muse serve` process died. */
    exit: () => exitOk({ code: 1, signal: null }),
    /** The family shut the process down. */
    closed: () => closedFlag,
    /** Feed one more server line, for scenarios the transcripts do not hold. */
    inject: (frame) => incoming.push(`${JSON.stringify(frame)}\n`),
  };
}
