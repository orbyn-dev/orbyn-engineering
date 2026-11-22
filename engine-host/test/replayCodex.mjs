import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import { PassThrough } from "node:stream";

// Replays a T3 Code codex_transcript.ndjson (MIT, Copyright (c) 2026 T3 Tools Inc., commit
// 9168ca73d02003557725430b515b44e1551544af; see test/fixtures/NOTICE) as a fake
// `codex app-server` child: expect_outbound entries are checked against the JSON-RPC lines the family
// writes to the child's stdin, emit_inbound entries are written to its stdout, and the pump stops at
// the next expected frame. T3's own requests (recorder config/read, thread naming, skills, the
// orchestration child threads) are not part of this family, so they are dropped together with their
// replies; the remaining frames are the wire contract between the host and the engine.
//
// The transcript's turn/start frames record T3's runtime mode, including approvalPolicy "never" and
// sandboxPolicy dangerFullAccess. Those fields are ignored here: the host never sends them and the
// tests assert the asking values separately.
const SKIP_LABEL_PREFIXES = ["child."];
const SKIP_METHODS = new Set([
  "config/read",
  "thread/name/set",
  "skills/list",
  "account/read",
  "thread/backgroundTerminals/list",
  "thread/backgroundTerminals/terminate",
]);
// Fields of turn/start that belong to T3's runtime mode and are deliberately replaced by this host.
const TURN_START_IGNORED = ["approvalPolicy", "approvalsReviewer", "sandboxPolicy", "collaborationMode", "additionalContext"];
// Fields of thread/start and its siblings that are T3's recorder config, not this host's.
const THREAD_IGNORED = ["approvalPolicy", "approvalsReviewer", "sandbox", "serviceName", "sessionStartSource", "threadSource"];

const sub = (value, workspace) => JSON.parse(JSON.stringify(value).replaceAll('"<workspace>"', JSON.stringify(workspace)));

function prepare(body) {
  const entries = [];
  const skippedIds = new Set();
  for (const entry of body) {
    if (entry.type === "expect_outbound") {
      const frame = entry.frame ?? {};
      const skip = SKIP_METHODS.has(frame.method) || SKIP_LABEL_PREFIXES.some((prefix) => (entry.label ?? "").startsWith(prefix));
      if (skip) {
        if (frame.id !== undefined) skippedIds.add(frame.id);
        continue;
      }
    } else if (entry.type === "emit_inbound" && entry.frame?.id !== undefined && skippedIds.has(entry.frame.id)) {
      skippedIds.delete(entry.frame.id);
      continue;
    }
    entries.push(entry);
  }
  return entries;
}

function checkMethod(expected, actual) {
  assert.equal(actual.method, expected.method, "method");
}

function checkThread(expected, actual) {
  const wanted = expected.params ?? {};
  const got = actual.params ?? {};
  for (const key of THREAD_IGNORED) assert.equal(got[key], undefined, `${expected.method}.${key} is T3's and must not be sent`);
  if (got.config !== undefined) assert.deepEqual(Object.keys(got.config).filter((key) => !key.startsWith("mcp_servers.")), [], "thread config only carries host MCP servers");
  for (const key of ["cwd", "model", "developerInstructions", "lastTurnId", "threadId", "excludeTurns", "baseInstructions", "ephemeral"]) {
    if (wanted[key] !== undefined) assert.deepEqual(got[key], wanted[key], `${expected.method}.${key}`);
  }
}

function check(expected, actual) {
  // A reply to an engine request: the decision belongs to the test, the id the engine is waiting for.
  if (expected.method === undefined) {
    assert.ok("result" in actual || "error" in actual, "server request reply carries a result");
    return;
  }
  checkMethod(expected, actual);
  const wanted = expected.params ?? {};
  const got = actual.params ?? {};
  switch (expected.method) {
    case "initialize":
      // clientInfo, T3's optOutNotificationMethods and its MCP UI extension are the client's own.
      assert.equal(actual.params?.capabilities?.experimentalApi, true, "experimentalApi");
      return;
    case "initialized":
      return;
    case "thread/start":
    case "thread/resume":
    case "thread/fork":
      checkThread(expected, actual);
      return;
    case "turn/start": {
      // approvalPolicy, approvalsReviewer, sandboxPolicy, collaborationMode and additionalContext are
      // T3's runtime mode; the host replaces them and each test asserts the asking values instead.
      assert.equal(got.threadId, wanted.threadId, "turn/start.threadId");
      assert.deepEqual(got.input, wanted.input, "turn/start.input");
      for (const key of ["cwd", "model", "effort", "clientUserMessageId", "serviceTier"]) {
        if (wanted[key] !== undefined) assert.deepEqual(got[key], wanted[key], `turn/start.${key}`);
      }
      return;
    }
    case "turn/steer":
      assert.equal(got.threadId, wanted.threadId, "turn/steer.threadId");
      assert.equal(got.expectedTurnId, wanted.expectedTurnId, "turn/steer.expectedTurnId");
      assert.deepEqual(got.input, wanted.input, "turn/steer.input");
      return;
    case "turn/interrupt":
      assert.equal(got.threadId, wanted.threadId, "turn/interrupt.threadId");
      assert.equal(got.turnId, wanted.turnId, "turn/interrupt.turnId");
      return;
    case "thread/read":
      assert.equal(got.threadId, wanted.threadId, "thread/read.threadId");
      if (wanted.includeTurns !== undefined) assert.equal(got.includeTurns, wanted.includeTurns, "thread/read.includeTurns");
      return;
    case "thread/turns/list":
      // The page size and cursor are the host's pagination; the order and view are the contract.
      assert.equal(got.threadId, wanted.threadId, "thread/turns/list.threadId");
      assert.equal(got.sortDirection, wanted.sortDirection, "thread/turns/list.sortDirection");
      assert.equal(got.itemsView, wanted.itemsView, "thread/turns/list.itemsView");
      return;
    case "thread/revert":
      assert.equal(got.threadId, wanted.threadId, "thread/revert.threadId");
      assert.equal(got.beforeTurnId, wanted.beforeTurnId, "thread/revert.beforeTurnId");
      return;
    default:
      assert.deepEqual(got, wanted, `${expected.method} params`);
  }
}

/** A fake `codex app-server` child process. */
function fakeChild(onFrame) {
  const child = new EventEmitter();
  child.pid = 4242;
  child.killed = false;
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  let remainder = "";
  child.stdin.setEncoding("utf8");
  child.stdin.on("data", (chunk) => {
    remainder += chunk;
    let newline;
    while ((newline = remainder.indexOf("\n")) >= 0) {
      const line = remainder.slice(0, newline).replace(/\r$/, "");
      remainder = remainder.slice(newline + 1);
      if (line.trim() === "") continue;
      onFrame(JSON.parse(line));
    }
  });
  child.kill = () => {
    if (child.killed) return true;
    child.killed = true;
    queueMicrotask(() => {
      child.stdout.end();
      child.emit("exit", 0, null);
    });
    return true;
  };
  return child;
}

export function replayCodex(source, { workspace = "/work" } = {}) {
  const lines = Array.isArray(source) ? source : readFileSync(source, "utf8").trim().split("\n").map((line) => JSON.parse(line));
  assert.equal(lines[0].type, "transcript_start");
  const header = lines[0];
  const body = prepare(lines.slice(1));
  const spawns = [];
  const failures = [];
  const outbound = [];
  const extra = [];
  let cursor = 0;
  let broken = false;
  let child;
  // T3's own requests take ids this host does not send, so recorded request ids are mapped to the
  // ids the family actually mints and rewritten on their replies.
  const requestIds = new Map();

  const writeFrame = (frame) => {
    const rewritten =
      frame?.id !== undefined && frame.method === undefined && requestIds.has(frame.id)
        ? { ...frame, id: requestIds.get(frame.id) }
        : frame;
    if (!child || child.stdout.destroyed) return;
    child.stdout.write(`${JSON.stringify(rewritten)}\n`);
  };

  const write = (line) => {
    if (!child || child.stdout.destroyed) return;
    child.stdout.write(line);
  };

  const pump = () => {
    while (cursor < body.length) {
      const entry = body[cursor];
      // A runtime_exit separates two app-server processes; the next expected frame belongs to the
      // next child this test starts.
      if (entry.type === "runtime_exit") {
        cursor++;
        continue;
      }
      if (entry.type !== "emit_inbound") break;
      cursor++;
      writeFrame(sub(entry.frame, workspace));
    }
  };

  const tolerate = (frame) => {
    // The host loads a resumed thread in `open`; T3 defers that load to this later rollback. The
    // replay acknowledges the extra request so the recorded thread/read still lines up.
    if (frame.method === "thread/resume" && body[cursor]?.type === "expect_outbound" && body[cursor].frame.method === "thread/read") {
      extra.push(frame);
      const recorded = body.slice(cursor).find((entry) => entry.type === "emit_inbound" && entry.frame?.result?.thread?.id);
      const threadId = recorded?.frame.result.thread.id;
      write(`${JSON.stringify({ id: frame.id, result: threadId ? { thread: { id: threadId } } : {} })}\n`);
      return true;
    }
    // Cancel can interrupt a turn the recording never interrupted (delegated_task_status leaves its
    // parent turn open); the transcript is exhausted by then.
    if (cursor >= body.length && frame.method === "turn/interrupt") {
      extra.push(frame);
      write(`${JSON.stringify({ id: frame.id, result: {} })}\n`);
      return true;
    }
    return false;
  };

  const onFrame = (frame) => {
    outbound.push(frame);
    // The host never grants full access and never answers a persistent allow, whatever the recording
    // held; every replay test enforces that on the wire.
    if (frame.method === "turn/start") {
      assert.ok(["on-request", "untrusted"].includes(frame.params?.approvalPolicy), "turn/start asks");
      assert.ok(["readOnly", "workspaceWrite"].includes(frame.params?.sandboxPolicy?.type), "turn/start sandbox");
    }
    if (frame.method === undefined) {
      assert.ok(!/acceptForSession|acceptAlways|approved_for_session|dangerFullAccess/.test(JSON.stringify(frame)), "no persistent allow");
    }
    pump();
    const entry = body[cursor];
    if (broken) return;
    if (tolerate(frame)) return;
    if (!entry) {
      broken = true;
      failures.push(`unexpected outbound ${frame.method ?? "reply"} after the transcript ended`);
      return;
    }
    try {
      assert.equal(entry.type, "expect_outbound", `unexpected outbound ${frame.method ?? "reply"}; next is ${entry.type}`);
      check(sub(entry.frame, workspace), frame);
    } catch (cause) {
      broken = true;
      failures.push(`${entry.label}: ${cause.message}`);
      return;
    }
    if (entry.frame?.id !== undefined && frame.id !== undefined) requestIds.set(entry.frame.id, frame.id);
    cursor++;
    pump();
  };

  const spawn = (options) => {
    spawns.push(options);
    child = fakeChild(onFrame);
    return child;
  };

  return {
    spawn,
    spawns,
    failures,
    outbound,
    extra,
    /** Expected frames the family never wrote (a complete test leaves none). */
    remaining: () => body.slice(cursor).filter((entry) => entry.type === "expect_outbound").map((entry) => entry.label),
    methods: () => outbound.map((frame) => frame.method ?? "reply"),
    promptTexts: () => body.filter((entry) => entry.type === "expect_outbound" && entry.frame.method === "turn/start").map((entry) => entry.frame.params.input.find((part) => part.type === "text")?.text ?? ""),
    model: () =>
      header.metadata?.model ??
      body.find((entry) => entry.type === "expect_outbound" && entry.frame.method === "turn/start")?.frame.params.model,
    sessionId: () => {
      const start = body.find((entry) => entry.type === "emit_inbound" && entry.frame?.result?.thread?.id);
      return start?.frame.result.thread.id;
    },
    /** The current fake child's exit. */
    exit: (code = 1) => {
      child.stdout.end();
      child.emit("exit", code, null);
    },
    /** The family shut the process down. */
    closed: () => child?.killed === true,
    /** Feed one more server line, for scenarios the transcripts do not hold. */
    inject: (frame) => writeFrame(frame),
  };
}
