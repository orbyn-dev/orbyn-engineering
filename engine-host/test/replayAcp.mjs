import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// Replays a T3 Code ACP registry transcript (MIT, Copyright (c) 2026 T3 Tools Inc., commit
// 9168ca73d02003557725430b515b44e1551544af, see test/fixtures/NOTICE) against the family's
// ACP client. The transcript is the recorded ndjson wire of one agent process: expect_outbound
// entries are checked against the lines the family writes, emit_inbound entries are fed back as
// the agent's lines, and the pump stops at the next expected frame. The recordings carry no ids
// and no JSON-RPC envelope, so this harness mints ids and pairs each recorded response with the
// host request of the same method. `"<any>"` stands for a value the host owns (T3's clientInfo,
// capabilities, prompt wrapper, mcpServers); `"<workspace>"` is replaced by the test's workspace.
//
// A transcript can hold more than one agent process (Kiro's fork recording does): each spawn gets
// its own line handler, and responses go back to the process whose request they answer.
//
// `ignoreExpectedOutbound` drops recorded frames the host deliberately does not write (and the
// response paired with them); `ignoreActualOutbound` accepts a host frame the transcript does not
// hold, optionally answering it through `respondToIgnoredRequest`. Tests using either must say
// why in a comment, and every ignored frame should be asserted on its own.

const ANY = "<any>";
const WORKSPACE = "<workspace>";

const sub = (value, workspace) => JSON.parse(JSON.stringify(value).replaceAll('"<workspace>"', JSON.stringify(workspace)));

/**
 * The core's user text inside a T3-recorded prompt block. T3 wraps the person's message in its
 * own `<t3_code_instructions>`/`<user_request>` envelope, which the host must not send; the
 * recording keeps only the wrapper and the user request. A block the host owns is compared on
 * the `<user_request>` body alone.
 */
export function recordedUserText(text) {
  if (typeof text !== "string") return text;
  const match = /<user_request>\r?\n?([\s\S]*?)\r?\n?<\/user_request>/.exec(text);
  return match ? match[1] : text;
}

/** The prompt blocks the host owns from a recorded prompt: T3's runtime_info block is ignored. */
function recordedPromptBlocks(prompt) {
  return (Array.isArray(prompt) ? prompt : []).flatMap((block) => {
    if (block?.type !== "text") return [block];
    const text = typeof block.text === "string" ? block.text : "";
    if (text.startsWith("<runtime_info>")) return [];
    return [{ type: "text", text: recordedUserText(text) }];
  });
}

/** Compare a recorded frame with the host's, honouring T3's two placeholders. */
function match(expected, actual, workspace, path = "params") {
  if (expected === ANY) return;
  if (expected === WORKSPACE) {
    assert.equal(actual, workspace, `${path}: workspace`);
    return;
  }
  if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${path}: expected an array`);
    assert.equal(actual.length, expected.length, `${path}: length`);
    expected.forEach((item, index) => match(item, actual[index], workspace, `${path}[${index}]`));
    return;
  }
  if (expected !== null && typeof expected === "object") {
    assert.ok(actual !== null && typeof actual === "object" && !Array.isArray(actual), `${path}: expected an object`);
    for (const [key, value] of Object.entries(expected)) {
      assert.ok(Object.hasOwn(actual, key), `${path}.${key}: missing`);
      match(value, actual[key], workspace, `${path}.${key}`);
    }
    return;
  }
  assert.deepEqual(actual, expected, path);
}

export function replayAcp(
  source,
  { workspace = "/work", ignoreExpectedOutbound, ignoreActualOutbound, respondToIgnoredRequest } = {},
) {
  const entries = Array.isArray(source) ? source : readFileSync(source, "utf8").trim().split("\n").map((line) => JSON.parse(line));
  assert.equal(entries[0].type, "transcript_start");
  assert.equal(entries[0].protocol, "acp.ndjson-jsonrpc");
  const recorded = entries.slice(1).filter((entry) => entry.type === "expect_outbound" || entry.type === "emit_inbound");
  // A recorded frame the host does not write also drops the response paired with it; otherwise
  // the pump would answer a request that never went out.
  const skipped = new Set();
  for (const [index, entry] of recorded.entries()) {
    if (entry.type !== "expect_outbound" || ignoreExpectedOutbound?.(entry.frame) !== true) continue;
    skipped.add(index);
    const method = entry.frame?.method;
    if (method === undefined) continue;
    for (let next = index + 1; next < recorded.length; next++) {
      if (recorded[next].type === "expect_outbound") break;
      if (recorded[next].type === "emit_inbound" && recorded[next].frame?.kind === "response" && recorded[next].frame?.method === method) {
        skipped.add(next);
        break;
      }
    }
  }
  const body = recorded.filter((_, index) => !skipped.has(index));

  const spawns = [];
  const failures = [];
  const outbound = [];
  const ignoredOutbound = [];
  const hostRequestMethods = new Map(); // our jsonrpc id -> method
  const hostRequestOrigins = new Map(); // our jsonrpc id -> the child that wrote it
  const hostRequestsByMethod = new Map(); // method -> [{id, origin}, oldest first]
  const agentRequests = new Map(); // the agent's request id -> {method, origin}
  const sessionHandlers = new Map(); // ACP sessionId -> the child that session belongs to
  const outboundResponses = []; // { method, result }
  let active; // the child whose line is being replayed now
  let exitResolve;
  let closedFlag = false;
  let cursor = 0;
  let broken = false;
  let nextAgentRequestId = 9000;

  const check = (entry, frame) => {
    const expected = sub(entry.frame, workspace);
    if (expected.kind === "response") {
      assert.equal(frame.error, undefined, `${entry.label}: unexpected error response`);
      assert.deepEqual(frame.result, expected.result, `${entry.label}: response result`);
      return;
    }
    assert.equal(frame.method, expected.method, `${entry.label}: method`);
    if (expected.params === undefined) return;
    // T3 negotiates ACP v2 and v1 in one initialize: `info`/`capabilities` are its v2 aliases
    // for this host's `clientInfo`/`clientCapabilities`, and T3 also advertises fs and terminal.
    // The host owns protocolVersion and the absence of fs/terminal (asserted in acp.test.mjs),
    // so those recorded fields are ignored here. Kiro likewise advertises T3's session
    // notices/compaction and its `_meta.kiro.hooks` capability; this host does not.
    if (expected.method === "initialize") {
      assert.equal(frame.params.protocolVersion, expected.params.protocolVersion, "initialize.protocolVersion");
      assert.equal(frame.params.clientInfo?.name, "Orbyn", "initialize.clientInfo.name");
      return;
    }
    if (expected.method === "session/prompt") {
      assert.equal(frame.params.sessionId, expected.params.sessionId, "session/prompt.sessionId");
      const wanted = recordedPromptBlocks(expected.params.prompt);
      const actual = frame.params.prompt;
      assert.ok(Array.isArray(actual), "session/prompt.prompt");
      assert.deepEqual(actual, wanted, "session/prompt.prompt (T3's instruction wrapper is not the host's)");
      return;
    }
    match(expected.params, frame.params, workspace, `${expected.method}.params`);
  };

  const feed = (frame, origin = active) => {
    assert.ok(origin?.line, "the family registered no line handler for this process");
    origin.line(JSON.stringify(frame));
  };

  const pump = () => {
    while (cursor < body.length && body[cursor].type === "emit_inbound") {
      const { label, frame } = body[cursor];
      const recordedFrame = sub(frame, workspace);
      if (recordedFrame.kind === "response") {
        const origins = hostRequestsByMethod.get(recordedFrame.method) ?? [];
        assert.ok(origins.length > 0, `${label}: no host request of method ${recordedFrame.method} to answer`);
        const { id, origin } = origins.shift();
        cursor++;
        feed(
          {
            jsonrpc: "2.0",
            id,
            ...(recordedFrame.result === undefined ? {} : { result: recordedFrame.result }),
            ...(recordedFrame.error === undefined ? {} : { error: recordedFrame.error }),
          },
          origin,
        );
        continue;
      }
      // A notification or agent request belongs to the process that owns the session it names.
      const origin = sessionHandlers.get(recordedFrame.params?.sessionId) ?? active;
      if (recordedFrame.kind === "request") {
        const id = nextAgentRequestId++;
        agentRequests.set(id, { method: recordedFrame.method, origin });
        cursor++;
        feed({ jsonrpc: "2.0", id, method: recordedFrame.method, params: recordedFrame.params }, origin);
        continue;
      }
      assert.equal(recordedFrame.kind, "notification", `${label}: kind`);
      cursor++;
      feed({ jsonrpc: "2.0", method: recordedFrame.method, params: recordedFrame.params }, origin);
    }
  };

  const onHostFrame = (frame, origin) => {
    active = origin;
    outbound.push(frame);
    if (typeof frame.params?.sessionId === "string") sessionHandlers.set(frame.params.sessionId, origin);
    if (broken) return;
    const entry = body[cursor];
    try {
      if (ignoreActualOutbound?.(frame) === true) {
        ignoredOutbound.push(frame);
        const result = respondToIgnoredRequest?.(frame);
        if (result !== undefined && frame.method !== undefined && frame.id !== undefined) {
          origin.line(JSON.stringify({ jsonrpc: "2.0", id: frame.id, result }));
        }
        return;
      }
      assert.ok(entry && entry.type === "expect_outbound", `unexpected outbound ${frame.method ?? "response"}; the transcript has ${entry ? entry.type : "ended"}`);
      if (frame.method !== undefined) {
        check(entry, frame);
      } else {
        const request = agentRequests.get(frame.id);
        assert.ok(request, `response ${frame.id}: no agent request`);
        const expected = sub(entry.frame, workspace);
        assert.equal(expected.kind, "response", `${entry.label}: expected a host request`);
        assert.equal(expected.method, request.method, `${entry.label}: response method`);
        assert.equal(frame.error, undefined, `${entry.label}: unexpected error response`);
        assert.deepEqual(frame.result, expected.result, `${entry.label}: response result`);
        outboundResponses.push({ method: request.method, result: frame.result });
      }
      if (frame.method !== undefined && frame.id !== undefined) {
        hostRequestMethods.set(frame.id, frame.method);
        hostRequestOrigins.set(frame.id, origin);
        const pending = hostRequestsByMethod.get(frame.method) ?? [];
        pending.push({ id: frame.id, origin });
        hostRequestsByMethod.set(frame.method, pending);
      }
    } catch (cause) {
      broken = true;
      failures.push(`${entry?.label ?? "?"}: ${cause.message}`);
      throw cause;
    }
    cursor++;
    pump();
  };

  const spawn = (options) => {
    spawns.push(options);
    let resolveExit;
    const exited = new Promise((resolve) => {
      resolveExit = resolve;
      exitResolve = resolve;
    });
    const origin = { line: undefined };
    return {
      write(line) {
        onHostFrame(JSON.parse(line), origin);
      },
      onLine(handler) {
        origin.line = handler;
      },
      onStderr(handler) {
        origin.stderr = handler;
      },
      onError() {},
      exited,
      async close() {
        closedFlag = true;
        resolveExit({ code: 0, signal: null });
      },
    };
  };

  const prompts = body.filter((entry) => entry.type === "expect_outbound" && entry.frame.method === "session/prompt");
  const sessionNew = body.filter((entry) => entry.type === "emit_inbound" && entry.frame.method === "session/new");

  return {
    spawn,
    spawns,
    failures,
    outbound,
    /** Host frames `ignoreActualOutbound` accepted instead of matching them. */
    ignoredOutbound,
    outboundResponses,
    /** Expected frames the family never wrote (a complete test leaves none). */
    remaining: () => body.slice(cursor).filter((entry) => entry.type === "expect_outbound").map((entry) => entry.label),
    /** Methods of the frames the family wrote, in order ("response" for an answer to the agent). */
    methods: () => outbound.map((frame) => frame.method ?? "response"),
    /** The core's user texts from the recorded prompts, in order. */
    promptTexts: () => prompts.map((entry) => recordedUserText(entry.frame.params.prompt.find((block) => block.type === "text")?.text)),
    /** The recorded session id of the `session/new` reply, first by default. */
    sessionId: (index = 0) => sessionNew[index]?.frame.result?.sessionId,
    /** The agent process died. */
    exit: (code = 1, signal = null) => exitResolve({ code, signal }),
    /** The family shut an agent process down. */
    closed: () => closedFlag,
    /** Play a line the recording does not hold (403 for a stray fs/terminal request, an ask, ...). */
    inject: (frame, origin = active) => feed(frame, origin),
    /** Push stderr text through the active family process's stderr handler. */
    stderr: (text) => active?.stderr?.(text),
    /** The options a frame of that method carried, newest first. */
    sent: (method) => outbound.filter((frame) => frame.method === method),
    /** Wait until the recording has been fully played (or fail the test). */
    done: () => {
      assert.deepEqual(failures, [], "replay mismatches");
      assert.deepEqual(
        body.slice(cursor).filter((entry) => entry.type === "expect_outbound").map((entry) => entry.label),
        [],
        "expected frames the family never wrote",
      );
    },
  };
}
