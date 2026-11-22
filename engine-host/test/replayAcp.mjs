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

export function replayAcp(source, { workspace = "/work" } = {}) {
  const entries = Array.isArray(source) ? source : readFileSync(source, "utf8").trim().split("\n").map((line) => JSON.parse(line));
  assert.equal(entries[0].type, "transcript_start");
  assert.equal(entries[0].protocol, "acp.ndjson-jsonrpc");
  const body = entries.slice(1).filter((entry) => entry.type === "expect_outbound" || entry.type === "emit_inbound");

  const spawns = [];
  const failures = [];
  const outbound = [];
  const hostRequestMethods = new Map(); // our jsonrpc id -> method
  const hostRequestsByMethod = new Map(); // method -> [our jsonrpc id, oldest first]
  const agentRequestMethods = new Map(); // the agent's request id -> method
  const outboundResponses = []; // { method, result }
  let lineHandler;
  let stderrHandler;
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
    // so those recorded fields are ignored here.
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

  const feed = (frame) => {
    assert.ok(lineHandler, "the family registered no line handler");
    lineHandler(JSON.stringify(frame));
  };

  const pump = () => {
    while (cursor < body.length && body[cursor].type === "emit_inbound") {
      const { label, frame } = body[cursor];
      const recorded = sub(frame, workspace);
      if (recorded.kind === "response") {
        const ids = hostRequestsByMethod.get(recorded.method) ?? [];
        assert.ok(ids.length > 0, `${label}: no host request of method ${recorded.method} to answer`);
        const id = ids.shift();
        cursor++;
        feed({
          jsonrpc: "2.0",
          id,
          ...(recorded.result === undefined ? {} : { result: recorded.result }),
          ...(recorded.error === undefined ? {} : { error: recorded.error }),
        });
        continue;
      }
      if (recorded.kind === "request") {
        const id = nextAgentRequestId++;
        agentRequestMethods.set(id, recorded.method);
        cursor++;
        feed({ jsonrpc: "2.0", id, method: recorded.method, params: recorded.params });
        continue;
      }
      assert.equal(recorded.kind, "notification", `${label}: kind`);
      cursor++;
      feed({ jsonrpc: "2.0", method: recorded.method, params: recorded.params });
    }
  };

  const onHostFrame = (frame) => {
    outbound.push(frame);
    if (broken) return;
    const entry = body[cursor];
    try {
      assert.ok(entry && entry.type === "expect_outbound", `unexpected outbound ${frame.method ?? "response"}; the transcript has ${entry ? entry.type : "ended"}`);
      if (frame.method !== undefined) {
        check(entry, frame);
      } else {
        const method = agentRequestMethods.get(frame.id);
        assert.ok(method, `response ${frame.id}: no agent request`);
        const expected = sub(entry.frame, workspace);
        assert.equal(expected.kind, "response", `${entry.label}: expected a host request`);
        assert.equal(expected.method, method, `${entry.label}: response method`);
        assert.equal(frame.error, undefined, `${entry.label}: unexpected error response`);
        assert.deepEqual(frame.result, expected.result, `${entry.label}: response result`);
        outboundResponses.push({ method, result: frame.result });
      }
      if (frame.method !== undefined && frame.id !== undefined) {
        hostRequestMethods.set(frame.id, frame.method);
        const ids = hostRequestsByMethod.get(frame.method) ?? [];
        ids.push(frame.id);
        hostRequestsByMethod.set(frame.method, ids);
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
    const exited = new Promise((resolve) => (exitResolve = resolve));
    return {
      write(line) {
        onHostFrame(JSON.parse(line));
      },
      onLine(handler) {
        lineHandler = handler;
      },
      onStderr(handler) {
        stderrHandler = handler;
      },
      onError() {},
      exited,
      async close() {
        closedFlag = true;
        exitResolve({ code: 0, signal: null });
      },
    };
  };

  const prompts = body.filter((entry) => entry.type === "expect_outbound" && entry.frame.method === "session/prompt");
  const sessionNew = body.find((entry) => entry.type === "emit_inbound" && entry.frame.method === "session/new");

  return {
    spawn,
    spawns,
    failures,
    outbound,
    outboundResponses,
    /** Expected frames the family never wrote (a complete test leaves none). */
    remaining: () => body.slice(cursor).filter((entry) => entry.type === "expect_outbound").map((entry) => entry.label),
    /** Methods of the frames the family wrote, in order ("response" for an answer to the agent). */
    methods: () => outbound.map((frame) => frame.method ?? "response"),
    /** The core's user texts from the recorded prompts, in order. */
    promptTexts: () => prompts.map((entry) => recordedUserText(entry.frame.params.prompt.find((block) => block.type === "text")?.text)),
    sessionId: () => sessionNew?.frame.result?.sessionId,
    /** The agent process died. */
    exit: (code = 1, signal = null) => exitResolve({ code, signal }),
    /** The family shut the agent process down. */
    closed: () => closedFlag,
    /** Play a line the recording does not hold (403 for a stray fs/terminal request, an ask, ...). */
    inject: (frame) => feed(frame),
    /** Push stderr text through the family's stderr handler. */
    stderr: (text) => stderrHandler?.(text),
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
