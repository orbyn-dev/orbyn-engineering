import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { runHost } from "../src/host.mjs";
import { acpFamily, acpLaunch, acpModelCatalog, ACP_PROTOCOL_VERSION, acpStderrExcerpt } from "../src/families/acp.mjs";
import { replayAcp } from "./replayAcp.mjs";

const fixture = (name) => fileURLToPath(new URL(`./fixtures/acp/${name}.ndjson`, import.meta.url));
const WORKSPACE = "/work";
const AGENT = { command: "fake-agent", args: [], env: {}, id: "default" };
const settle = (ms = 40) => new Promise((resolve) => setTimeout(resolve, ms));
const runs = (h, sendId) => h.all.filter((frame) => frame.type === "runDone" && frame.sendId === sendId);

function boot({ spawn, ...familyOptions } = {}) {
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const all = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    const frame = JSON.parse(line);
    all.push(frame);
    frames.push(frame);
    for (const waiter of [...waiters]) waiter();
  });
  runHost({ family: acpFamily({ spawn, ...familyOptions }), input, output });
  const next = (predicate) =>
    new Promise((resolve) => {
      const check = () => {
        const index = frames.findIndex(predicate);
        if (index < 0) return false;
        resolve(frames.splice(index, 1)[0]);
        const indexOfCheck = waiters.indexOf(check);
        if (indexOfCheck >= 0) waiters.splice(indexOfCheck, 1);
        return true;
      };
      if (!check()) waiters.push(check);
    });
  const send = (frame) => input.write(`${JSON.stringify(frame)}\n`);
  return { next, send, all, input };
}

async function handshake(h) {
  await h.next((frame) => frame.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((frame) => frame.id === 1);
}

async function open(h, extra = {}, id = 2, sessionId = "s") {
  h.send({
    id,
    type: "open",
    operation: "create",
    sessionId,
    cwd: WORKSPACE,
    options: { agent: AGENT, ...extra.options },
    ...extra.request,
  });
  return h.next((frame) => frame.id === id);
}

function scenario(name) {
  const replay = replayAcp(fixture(name), { workspace: WORKSPACE });
  const h = boot({ spawn: replay.spawn });
  return { replay, h };
}

async function ready(name, extra = {}) {
  const { replay, h } = scenario(name);
  await handshake(h);
  const opened = await open(h, extra);
  assert.equal(opened.ok, true, JSON.stringify(opened));
  assert.equal(opened.result.engineSessionId, replay.sessionId());
  return { replay, h, opened };
}

function deltas(h, sendId) {
  return h.all.filter((frame) => frame.type === "delta" && frame.sendId === sendId);
}

async function turn(h, replay, sendIndex, sessionId = "s") {
  const text = replay.promptTexts()[sendIndex];
  h.send({ id: 3 + sendIndex * 10, type: "send", sessionId, message: text });
  const accepted = await h.next((frame) => frame.id === 3 + sendIndex * 10);
  assert.equal(accepted.ok, true, JSON.stringify(accepted));
  return { sendId: 3 + sendIndex * 10, text };
}

// ---------------------------------------------------------------------------------------------
// Recorded transcripts. Each test drives the host protocol and the replay harness feeds the
// recorded agent wire; the harness checks every frame the host writes.
// ---------------------------------------------------------------------------------------------

test("hello names the family, the ACP protocol version and the requests it implements", async () => {
  const { h, replay } = scenario("simple");
  const hello = await h.next((frame) => frame.type === "hello");
  assert.equal(hello.family, "acp");
  assert.equal(hello.sdk.name, "agent-client-protocol");
  assert.equal(hello.sdk.version, String(ACP_PROTOCOL_VERSION));
  assert.deepEqual(hello.capabilities, ["steer", "models"]);
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((frame) => frame.id === 1);
  await open(h);
  const initialize = replay.sent("initialize")[0];
  assert.equal(initialize.params.protocolVersion, 2);
  assert.equal(initialize.params.clientInfo.name, "Orbyn");
  // The agent keeps its own tools under the core's confinement: this host offers no filesystem
  // and no terminal, deliberately unlike T3 (clientPolicy/clientTerminals).
  assert.equal(initialize.params.clientCapabilities.fs, undefined);
  assert.equal(initialize.params.clientCapabilities.terminal, undefined);
  assert.deepEqual(initialize.params.clientCapabilities.elicitation, { form: {} });
  await settle();
  assert.deepEqual(replay.failures, []);
});

test("simple: thoughts and message chunks pass through untouched and one run completes", async () => {
  const { replay, h } = await ready("simple");
  const { sendId } = await turn(h, replay, 0);
  const accepted = h.all.find((frame) => frame.id === sendId);
  assert.equal(accepted.result.engineSessionId, replay.sessionId());
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.ok, true);
  assert.deepEqual(done.result, { reason: "completed", threadDisposition: "reusable" });
  const updates = deltas(h, sendId).map((frame) => frame.update);
  assert.deepEqual(
    updates.map((update) => update.sessionUpdate),
    ["agent_thought_chunk", "agent_thought_chunk", "agent_message_chunk", "agent_message_chunk"],
  );
  assert.deepEqual(updates[2], {
    sessionUpdate: "agent_message_chunk",
    content: { type: "text", text: "fixture simple " },
  });
  await settle();
  assert.equal(runs(h, sendId).length, 1);
  replay.done();
});

test("multi_turn: one process serves both prompts and each send ends exactly once", async () => {
  const { replay, h } = await ready("multi_turn");
  const first = await turn(h, replay, 0);
  const firstDone = await h.next((frame) => frame.type === "runDone" && frame.sendId === first.sendId);
  assert.equal(firstDone.result.reason, "completed");
  const second = await turn(h, replay, 1);
  const secondDone = await h.next((frame) => frame.type === "runDone" && frame.sendId === second.sendId);
  assert.equal(secondDone.result.reason, "completed");
  await settle();
  assert.equal(runs(h, first.sendId).length, 1);
  assert.equal(runs(h, second.sendId).length, 1);
  assert.equal(replay.sent("session/prompt").length, 2);
  replay.done();
});

test("queued_turn: the second prompt is a new send after the first run ended", async () => {
  const { replay, h } = await ready("queued_turn");
  const first = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === first.sendId);
  const second = await turn(h, replay, 1);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === second.sendId);
  await settle();
  assert.deepEqual(replay.sent("session/prompt").map((frame) => frame.params.prompt), [
    [{ type: "text", text: "Respond with exactly: first fixture turn complete" }],
    [{ type: "text", text: "Respond with exactly: second fixture turn complete" }],
  ]);
  replay.done();
});

test("message_steering: steer cancels the prompt, prompts the new text, and the run ends once", async () => {
  const { replay, h } = await ready("message_steering");
  const first = await turn(h, replay, 0);
  const partial = await h.next((frame) => frame.type === "delta" && frame.sendId === first.sendId);
  assert.equal(partial.update.content.text, "ste");
  h.send({ id: 40, type: "steer", sendId: first.sendId, messageId: "m-1", text: replay.promptTexts()[1] });
  const steered = await h.next((frame) => frame.id === 40);
  assert.deepEqual(steered.result, { outcome: "accepted", messageId: "m-1" });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === first.sendId);
  assert.equal(done.result.reason, "completed");
  assert.equal(done.ok, true);
  await settle();
  assert.equal(runs(h, first.sendId).length, 1, "a steer never ends the run");
  assert.deepEqual(replay.methods().filter((method) => method === "session/cancel"), ["session/cancel"]);
  assert.deepEqual(replay.sent("session/prompt")[1].params.prompt, [
    { type: "text", text: "Actually, respond with exactly: steering fixture observed" },
  ]);
  replay.done();
});

test("tool_call_read_only: tool calls stream as deltas and the turn completes", async () => {
  const { replay, h } = await ready("tool_call_read_only");
  const { sendId } = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  const updates = deltas(h, sendId).map((frame) => frame.update);
  assert.deepEqual(updates.map((update) => update.sessionUpdate), [
    "agent_message_chunk",
    "tool_call",
    "tool_call_update",
    "tool_call_update",
    "tool_call",
    "tool_call_update",
    "agent_message_chunk",
  ]);
  assert.equal(updates[1].toolCallId, "read-package");
  assert.deepEqual(updates[1].rawInput, { path: "/tmp/claude-replay-tool_call_read_only/package.json" });
  assert.deepEqual(updates[3].rawOutput, { content: '{"name":"cursor-read-only-fixture"}' });
  replay.done();
});

test("tool_call_read_only_on_request: the ask is a permission frame and allow is allow_once", async () => {
  const { replay, h } = await ready("tool_call_read_only_on_request");
  const { sendId } = await turn(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission" && frame.sendId === sendId);
  assert.equal(ask.tool, "Write probe file");
  assert.equal(ask.input.toolCall.toolCallId, "write-probe");
  // The request named the call by id only; the tracked tool_call supplies the input (T3:
  // withTrackedPermissionToolCall).
  assert.deepEqual(ask.input.toolCall.rawInput, {
    command: "printf 'codex app-server approval fixture' > .codex-probe-write-action.txt",
  });
  assert.deepEqual(ask.options.map((option) => option.kind), ["allow_once", "allow_always", "reject_once"]);
  assert.deepEqual(ask.options.map((option) => option.optionId), ["allow-once", "allow-always", "reject-once"]);
  h.send({ id: 30, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((frame) => frame.id === 30)).result, null);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, sendId).length, 1);
  assert.deepEqual(replay.outboundResponses, [
    { method: "session/request_permission", result: { outcome: { outcome: "selected", optionId: "allow-once" } } },
  ]);
  replay.done();
});

test("turn_interrupt: cancel sends session/cancel and the run ends cancelled exactly once", async () => {
  const { replay, h } = await ready("turn_interrupt");
  const { sendId } = await turn(h, replay, 0);
  h.send({ id: 50, type: "cancel", sendId });
  assert.deepEqual((await h.next((frame) => frame.id === 50)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.deepEqual(done.result, { reason: "cancelled", threadDisposition: "reusable" });
  await settle();
  assert.equal(runs(h, sendId).length, 1);
  replay.done();
});

test("todo_list: plan updates pass through and the turn completes", async () => {
  const { replay, h } = await ready("todo_list");
  const { sendId } = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  const plans = deltas(h, sendId).filter((frame) => frame.update.sessionUpdate === "plan");
  assert.equal(plans.length, 2);
  assert.deepEqual(plans[0].update.entries.map((entry) => [entry.content, entry.status]), [
    ["Inspect package.json", "in_progress"],
    ["Inspect tsconfig.json", "pending"],
    ["Report completion", "pending"],
  ]);
  assert.deepEqual(plans[1].update.entries.map((entry) => entry.status), ["completed", "completed", "completed"]);
  replay.done();
});

test("acp_elicitation: a form becomes a question frame and the answer is the recorded accept", async () => {
  const { replay, h } = await ready("acp_elicitation");
  const { sendId } = await turn(h, replay, 0);
  const question = await h.next((frame) => frame.type === "question" && frame.sendId === sendId);
  assert.equal(question.tool, "AskUserQuestion");
  const asked = question.input.questions[0];
  assert.equal(asked.question, "Should this fixture prefer strict schemas or UI flexibility?");
  assert.equal(asked.header, "Fixture");
  assert.deepEqual(asked.options.map((option) => option.label), ["Strict schemas (Recommended)", "UI flexibility"]);
  h.send({ id: 60, type: "respond", requestId: question.requestId, decision: { answers: { [asked.question]: "Strict schemas (Recommended)" } } });
  assert.deepEqual((await h.next((frame) => frame.id === 60)).result, null);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.result.reason, "completed");
  await settle();
  replay.done();
});

test("acp_titled_multiselect: comma-joined answers map back to the recorded titles and values", async () => {
  const { replay, h } = await ready("acp_titled_multiselect");
  const { sendId } = await turn(h, replay, 0);
  const question = await h.next((frame) => frame.type === "question" && frame.sendId === sendId);
  const asked = question.input.questions[0];
  assert.equal(asked.multiSelect, true);
  assert.deepEqual(asked.options.map((option) => [option.label, option.description]), [
    ["Strict schemas", "Typed wire shapes"],
    ["UI flexibility", "UI flexibility"],
    ["Docs", "Docs"],
  ]);
  h.send({ id: 70, type: "respond", requestId: question.requestId, decision: { answers: { [asked.question]: "UI flexibility, Strict schemas" } } });
  assert.deepEqual((await h.next((frame) => frame.id === 70)).result, null);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.deepEqual(replay.outboundResponses, [
    { method: "elicitation/create", result: { action: "accept", content: { fixture_areas: ["ui", "schemas"] } } },
  ]);
  replay.done();
});

test("malformed_session_update: a malformed update is logged by method only and never kills the run", async () => {
  const { replay, h } = await ready("malformed_session_update");
  const { sendId } = await turn(h, replay, 0);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.result.reason, "completed");
  const updates = deltas(h, sendId).map((frame) => frame.update);
  assert.equal(updates.filter((update) => update.sessionUpdate === "usage_update").length, 1);
  assert.equal(updates.find((update) => update.sessionUpdate === "usage_update").used, 1200);
  const log = h.all.find((frame) => frame.type === "log" && frame.event.type === "malformedSessionUpdate");
  assert.ok(log, "malformed update logged");
  assert.deepEqual(log.event, { type: "malformedSessionUpdate", method: "session/update" });
  assert.ok(!JSON.stringify(log).includes("cost"), "the malformed content is never logged");
  replay.done();
});

test("stop_background_work_after_failed_turn: a prompt error ends the run failed once", async () => {
  const { replay, h } = await ready("stop_background_work_after_failed_turn");
  const { sendId } = await turn(h, replay, 0);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId);
  assert.equal(done.ok, false);
  assert.equal(done.result.reason, "failed");
  assert.equal(done.result.threadDisposition, "reusable");
  assert.equal(done.error.method, "send");
  assert.equal(done.error.cause.kind, "invalid_request");
  assert.equal(done.error.cause.code, -32603);
  await settle();
  assert.equal(runs(h, sendId).length, 1);
  replay.done();
});

// ---------------------------------------------------------------------------------------------
// Synthetic wire, for behaviour the recordings do not hold. The head below is the recorded
// session framing of the transcripts; the rest is written here.
// ---------------------------------------------------------------------------------------------

const START = { type: "transcript_start", provider: "acpRegistry", protocol: "acp.ndjson-jsonrpc", version: "1", scenario: "synthetic", metadata: {} };
const request = (label, method, params) => ({ type: "expect_outbound", label, frame: { kind: "request", method, params } });
const answer = (label, method, result) => ({ type: "expect_outbound", label, frame: { kind: "response", method, result } });
const notification = (label, method, params) => ({ type: "expect_outbound", label, frame: { kind: "notification", method, params } });
const reply = (label, method, result) => ({ type: "emit_inbound", label, frame: { kind: "response", method, result } });
const incoming = (label, method, params) => ({ type: "emit_inbound", label, frame: { kind: "notification", method, params } });
const ask = (label, method, params) => ({ type: "emit_inbound", label, frame: { kind: "request", method, params } });

function sessionHead({ init = {}, session = {} } = {}) {
  return [
    request("initialize", "initialize", { protocolVersion: 2, clientInfo: "<any>", clientCapabilities: "<any>" }),
    reply("initialized", "initialize", {
      protocolVersion: 1,
      agentCapabilities: { loadSession: true, promptCapabilities: { image: false, embeddedContext: false } },
      authMethods: [],
      ...init,
    }),
    request("session.new", "session/new", { cwd: "<workspace>", mcpServers: "<any>" }),
    reply("session.created", "session/new", { sessionId: "s1", ...session }),
  ];
}

const promptLine = (text = "hi") => request("prompt", "session/prompt", { sessionId: "s1", prompt: [{ type: "text", text }] });

function synthetic(body) {
  const replay = replayAcp([START, ...body], { workspace: WORKSPACE });
  const h = boot({ spawn: replay.spawn });
  return { replay, h };
}

async function syntheticReady(body) {
  const { replay, h } = synthetic(body);
  await handshake(h);
  const opened = await open(h);
  assert.equal(opened.ok, true, JSON.stringify(opened));
  return { replay, h };
}

test("a stray fs or terminal request is refused with method-not-found and only its method is logged", async () => {
  const frames = [];
  const agent = scripted({
    onRequest(frame) {
      frames.push(frame);
      if (frame.method === "initialize") agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1" });
      if (frame.method === "session/prompt") {
        agent.request(701, "fs/read_text_file", { sessionId: "s1", path: "/etc/passwd" });
        agent.request(702, "terminal/create", { sessionId: "s1", command: "id" });
      }
      if (frame.method === undefined && frame.id === 702) {
        agent.notify("session/update", { sessionId: "s1", update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "ok" } } });
        agent.respond(frames.find((item) => item.method === "session/prompt").id, { stopReason: "end_turn" });
      }
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  assert.equal((await open(h)).ok, true);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.type === "runDone");
  const response = agent.state.written.find((frame) => frame.method === undefined && frame.id === 701);
  assert.equal(response.error.code, -32601);
  assert.equal(agent.state.written.find((frame) => frame.method === "initialize").params.clientCapabilities.fs, undefined);
  assert.equal(agent.state.written.find((frame) => frame.method === "initialize").params.clientCapabilities.terminal, undefined);
  const log = h.all.find((frame) => frame.type === "log" && frame.event.type === "unsupportedAgentRequest");
  assert.deepEqual(log.event, { type: "unsupportedAgentRequest", method: "fs/read_text_file" });
  assert.deepEqual(agent.state.closed, 0);
});

test("confinement prefixes the command, replaces the environment and merges only the agent's env", async () => {
  process.env.ORBYN_TEST_LEAK = "leak";
  const agent = replayAcp(fixture("simple"), { workspace: WORKSPACE });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  const confinement = { argvPrefix: ["/usr/bin/sandbox-exec", "-f", "/p.sb", "--"], env: { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1" }, cwd: WORKSPACE, writeRoots: [WORKSPACE] };
  const opened = await open(h, {
    options: { agent: { command: "/opt/agent", args: ["--acp"], env: { AGENT_VAR: "1" } } },
    request: { confinement },
  });
  assert.equal(opened.ok, true, JSON.stringify(opened));
  const [spawn] = agent.spawns;
  assert.equal(spawn.command, "/usr/bin/sandbox-exec");
  assert.deepEqual(spawn.args, ["-f", "/p.sb", "--", "/opt/agent", "--acp"]);
  assert.deepEqual(spawn.env, { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1", AGENT_VAR: "1" });
  assert.equal(spawn.cwd, WORKSPACE);
  delete process.env.ORBYN_TEST_LEAK;
  assert.deepEqual(agent.failures, []);
});

test("open refusals start nothing: no agent, bad prefix, missing resume/fork support", async () => {
  const agent = scripted({ onRequest() {} });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  const refuse = async (id, request) => {
    h.send({ id, type: "open", operation: "create", sessionId: `s${id}`, cwd: WORKSPACE, ...request });
    return h.next((frame) => frame.id === id);
  };
  const noAgent = await refuse(2, { options: {} });
  assert.equal(noAgent.error.cause.kind, "invalid_request");
  const badPrefix = await refuse(3, { options: { agent: AGENT }, confinement: { argvPrefix: ["ok", ""] } });
  assert.equal(badPrefix.error.cause.kind, "invalid_request");
  const resume = await refuse(4, { operation: "resume", options: { agent: AGENT } });
  assert.equal(resume.error.cause.kind, "invalid_request");
  assert.equal(agent.state.spawned, 0);
  agent.done();
});

test("resume uses session/load only when the agent advertises it, otherwise unsupported", async () => {
  const load = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") load.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true, promptCapabilities: {} }, authMethods: [] });
      if (frame.method === "session/load") load.respond(frame.id, { configOptions: [] });
    },
  });
  const h = boot({ spawn: load.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "resume", engineSessionId: "old-session", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT } });
  const opened = await h.next((frame) => frame.id === 2);
  assert.deepEqual(opened.result, { engineSessionId: "old-session" });
  assert.deepEqual(load.state.written.find((frame) => frame.method === "session/load").params, {
    sessionId: "old-session",
    cwd: WORKSPACE,
    mcpServers: [],
  });
  await settle();

  const noLoad = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") noLoad.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: false }, authMethods: [] });
    },
  });
  const other = boot({ spawn: noLoad.spawn });
  await handshake(other);
  other.send({ id: 2, type: "open", operation: "resume", engineSessionId: "old-session", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT } });
  const refused = await other.next((frame) => frame.id === 2);
  assert.equal(refused.error.cause.kind, "unsupported");
  await settle();
  assert.equal(noLoad.state.closed, 1, "the unused process was stopped");
});

test("fork opens only when the agent advertises session/fork", async () => {
  const fork = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") fork.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true, sessionCapabilities: { fork: {} } }, authMethods: [] });
      if (frame.method === "session/fork") fork.respond(frame.id, { sessionId: "forked-session" });
    },
  });
  const h = boot({ spawn: fork.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "fork", engineSessionId: "old-session", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT } });
  assert.deepEqual((await h.next((frame) => frame.id === 2)).result, { engineSessionId: "forked-session" });
  assert.deepEqual(fork.state.written.find((frame) => frame.method === "session/fork").params, {
    sessionId: "old-session",
    cwd: WORKSPACE,
    mcpServers: [],
  });

  const noFork = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") noFork.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
    },
  });
  const other = boot({ spawn: noFork.spawn });
  await handshake(other);
  other.send({ id: 2, type: "open", operation: "fork", engineSessionId: "old-session", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT } });
  assert.equal((await other.next((frame) => frame.id === 2)).error.cause.kind, "unsupported");
  await settle();
  assert.equal(noFork.state.closed, 1);
});

test("an agent that requires sign-in fails open as auth unless the options name a method", async () => {
  const denied = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") denied.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [{ id: "login", name: "Log in" }] });
      if (frame.method === "session/new") denied.fail(frame.id, { code: -32000, message: "Authentication required" });
    },
  });
  const h = boot({ spawn: denied.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT } });
  const refused = await h.next((frame) => frame.id === 2);
  assert.equal(refused.error.cause.kind, "auth");
  await settle();
  assert.equal(denied.state.closed, 1);

  const allowed = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") allowed.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [{ id: "login", name: "Log in" }] });
      if (frame.method === "authenticate") {
        assert.deepEqual(frame.params, { methodId: "login" });
        allowed.respond(frame.id, {});
      }
      if (frame.method === "session/new") {
        if (allowed.state.authenticated !== true) allowed.fail(frame.id, { code: -32000, message: "Authentication required" });
        else allowed.respond(frame.id, { sessionId: "s1" });
      }
    },
  });
  const other = boot({ spawn: allowed.spawn });
  await handshake(other);
  other.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT, authMethodId: "login" } });
  // The handler above flips `authenticated` when it sees the authenticate call; patch it in.
  const opened = await other.next((frame) => frame.id === 2);
  assert.equal(opened.ok, true, JSON.stringify(opened));
  assert.ok(allowed.state.written.some((frame) => frame.method === "authenticate"));
});

test("mcpServers convert to ACP servers, and transport the agent does not support is skipped", async () => {
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") {
        agent.respond(frame.id, {
          protocolVersion: 1,
          agentCapabilities: { loadSession: true, mcpCapabilities: { http: true, sse: true } },
          authMethods: [],
        });
      }
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1" });
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  const mcpServers = [
    { name: "orbyn", type: "http", url: "http://127.0.0.1:7331/mcp", headers: { Authorization: "Bearer token" } },
    { name: "echo", command: "/bin/echo", args: ["hi"], env: { A: "1" } },
    { name: "acp-only", type: "acp", serverId: "acp-only" },
  ];
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT, mcpServers } });
  assert.equal((await h.next((frame) => frame.id === 2)).ok, true);
  const sent = agent.state.written.find((frame) => frame.method === "session/new");
  assert.deepEqual(sent.params.mcpServers, [
    { name: "orbyn", type: "http", url: "http://127.0.0.1:7331/mcp", headers: [{ name: "Authorization", value: "Bearer token" }] },
    { name: "echo", command: "/bin/echo", args: ["hi"], env: [{ name: "A", value: "1" }] },
  ]);
  const skipped = h.all.find((frame) => frame.type === "log" && frame.event.type === "mcpServerSkipped");
  assert.deepEqual(skipped.event, { type: "mcpServerSkipped", name: "acp-only", transport: "acp", sessionId: "s" });
});

test("permission answers: allow is allow_once, deny is reject_once, and always-only allow is denied", async () => {
  const cases = [
    {
      name: "allow-once",
      options: [
        { optionId: "always", name: "Always", kind: "allow_always" },
        { optionId: "once", name: "Once", kind: "allow_once" },
        { optionId: "reject", name: "No", kind: "reject_once" },
      ],
      decision: { behavior: "allow" },
      expected: { outcome: { outcome: "selected", optionId: "once" } },
    },
    {
      name: "deny",
      options: [
        { optionId: "once", name: "Once", kind: "allow_once" },
        { optionId: "reject", name: "No", kind: "reject_once" },
      ],
      decision: { behavior: "deny", message: "no" },
      expected: { outcome: { outcome: "selected", optionId: "reject" } },
    },
    {
      name: "always-only",
      options: [{ optionId: "always", name: "Always", kind: "allow_always" }],
      decision: { behavior: "allow" },
      expected: { outcome: { outcome: "cancelled" } },
    },
    {
      name: "always-only with reject",
      options: [
        { optionId: "always", name: "Always", kind: "allow_always" },
        { optionId: "reject", name: "No", kind: "reject_once" },
      ],
      decision: { behavior: "allow" },
      expected: { outcome: { outcome: "selected", optionId: "reject" } },
    },
  ];
  for (const [index, entry] of cases.entries()) {
    const body = [
      ...sessionHead(),
      promptLine(),
      ask("permission", "session/request_permission", {
        sessionId: "s1",
        toolCall: { toolCallId: "t1", title: "Write file", kind: "edit", status: "pending" },
        options: entry.options,
      }),
      answer("permission.response", "session/request_permission", entry.expected),
      reply("done", "session/prompt", { stopReason: "end_turn" }),
    ];
    const { replay, h } = await syntheticReady(body);
    h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
    const frame = await h.next((item) => item.type === "permission");
    h.send({ id: 40 + index, type: "respond", requestId: frame.requestId, decision: entry.decision });
    assert.equal((await h.next((item) => item.id === 40 + index)).ok, true);
    await h.next((item) => item.type === "runDone");
    replay.done();
  }
});

test("send options apply the selection before the prompt on the same run", async () => {
  const configOptions = [
    { id: "model", name: "Model", category: "model", type: "select", currentValue: "m", options: [{ value: "m", name: "M" }] },
    { id: "thought", name: "Thinking", category: "thought_level", type: "select", currentValue: "high", options: [{ value: "low", name: "Low" }, { value: "high", name: "High" }] },
  ];
  const updated = configOptions.map((option) => (option.id === "thought" ? { ...option, currentValue: "low" } : option));
  const body = [
    ...sessionHead({ session: { configOptions } }),
    request("effort", "session/set_config_option", { sessionId: "s1", configId: "thought", value: "low" }),
    reply("effort.done", "session/set_config_option", { configOptions: updated }),
    promptLine(),
    reply("done", "session/prompt", { stopReason: "end_turn" }),
  ];
  const { replay, h } = await syntheticReady(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi", options: { effort: "low" } });
  assert.equal((await h.next((frame) => frame.id === 3)).ok, true);
  await h.next((frame) => frame.type === "runDone");
  assert.deepEqual(
    replay.outbound.filter((frame) => frame.method === "session/set_config_option" || frame.method === "session/prompt").map((frame) => frame.method),
    ["session/set_config_option", "session/prompt"],
  );
  replay.done();
});

test("a denied question answers the agent with cancel and the run carries on", async () => {
  const body = [
    ...sessionHead(),
    promptLine(),
    ask("elicitation", "elicitation/create", {
      sessionId: "s1",
      mode: "form",
      message: "Pick one.",
      requestedSchema: {
        type: "object",
        properties: { choice: { type: "string", title: "Choice", enum: ["a", "b"] } },
        required: ["choice"],
      },
    }),
    answer("elicitation.cancel", "elicitation/create", { action: "cancel" }),
    reply("done", "session/prompt", { stopReason: "end_turn" }),
  ];
  const { replay, h } = await syntheticReady(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  const question = await h.next((frame) => frame.type === "question");
  h.send({ id: 4, type: "respond", requestId: question.requestId, decision: { behavior: "deny" } });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, null);
  const done = await h.next((frame) => frame.type === "runDone");
  assert.equal(done.result.reason, "completed");
  await settle();
  replay.done();
});

test("a late respond is ignored and logged, and never reaches the agent", async () => {
  const body = sessionHead();
  const { replay, h } = await syntheticReady(body);
  h.send({ id: 3, type: "respond", requestId: "p-99", decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((frame) => frame.id === 3)).result, null);
  assert.ok(h.all.some((frame) => frame.type === "log" && frame.event.type === "lateRespond"));
  assert.equal(replay.outbound.filter((frame) => frame.method === undefined).length, 0);
  replay.done();
});

test("cancel refuses the pending ask with the cancelled outcome before session/cancel", async () => {
  const body = [
    ...sessionHead(),
    promptLine(),
    ask("permission", "session/request_permission", {
      sessionId: "s1",
      toolCall: { toolCallId: "t1", title: "Run", kind: "execute" },
      options: [{ optionId: "once", name: "Once", kind: "allow_once" }],
    }),
    answer("permission.cancelled", "session/request_permission", { outcome: { outcome: "cancelled" } }),
    notification("cancel", "session/cancel", { sessionId: "s1" }),
    reply("cancelled", "session/prompt", { stopReason: "cancelled" }),
  ];
  const { replay, h } = await syntheticReady(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.type === "permission");
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone");
  assert.deepEqual(done.result, { reason: "cancelled", threadDisposition: "reusable" });
  await settle();
  assert.equal(runs(h, 3).length, 1);
  replay.done();
});

test("the process exit fails the run broken and stderr is only a redacted excerpt", async () => {
  let promptId;
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1" });
      if (frame.method === "session/prompt") promptId = frame.id;
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  assert.equal((await open(h)).ok, true);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.id === 3);
  agent.stderr(`prompt text that must not leak\nAuthorization: Bearer super-secret-token\nkey=sk-abcdefghijklmnop\n`);
  agent.exit(1);
  const done = await h.next((frame) => frame.type === "runDone");
  assert.equal(done.ok, false);
  assert.equal(done.result.threadDisposition, "broken");
  assert.equal(done.error.cause.kind, "engine");
  assert.match(done.error.cause.message, /Bearer \[redacted\]/);
  assert.match(done.error.cause.message, /\[redacted\]/);
  assert.ok(!JSON.stringify(h.all).includes("super-secret-token"), "no token in any frame");
  assert.ok(!JSON.stringify(h.all).includes("sk-abcdefghijklmnop"), "no key in any frame");
  void promptId;
  h.send({ id: 4, type: "send", sessionId: "s", message: "again" });
  assert.equal((await h.next((frame) => frame.id === 4)).error.cause.kind, "session_lost");
});

test("model, effort and mode selections at open go through the session config, then the prompt", async () => {
  const configOptions = [
    { id: "model", name: "Model", category: "model", type: "select", currentValue: "grok-build", options: [{ value: "grok-build", name: "Grok Build" }, { value: "grok-2", name: "Grok 2" }] },
    { id: "thought", name: "Thinking", category: "thought_level", type: "select", currentValue: "high", options: [{ value: "low", name: "Low" }, { value: "high", name: "High" }] },
    { id: "mode", name: "Mode", category: "mode", type: "select", currentValue: "build", options: [{ value: "build", name: "Build" }, { value: "plan", name: "Plan" }] },
  ];
  let current = configOptions;
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1", configOptions: current });
      if (frame.method === "session/set_config_option") {
        current = current.map((item) => (item.id === frame.params.configId ? { ...item, currentValue: frame.params.value } : item));
        agent.respond(frame.id, { configOptions: current });
      }
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT, model: "grok-2", effort: "low", mode: "plan" } });
  assert.equal((await h.next((frame) => frame.id === 2)).ok, true);
  assert.deepEqual(
    agent.state.written.filter((frame) => frame.method === "session/set_config_option").map((frame) => [frame.params.configId, frame.params.value]),
    [["model", "grok-2"], ["thought", "low"], ["mode", "plan"]],
  );
  h.send({ id: 3, type: "models", sessionId: "s" });
  const catalog = await h.next((frame) => frame.id === 3);
  assert.equal(catalog.ok, true, JSON.stringify(catalog));
  assert.deepEqual(catalog.result.models.map((model) => [model.id, model.name, model.isDefault]), [
    ["grok-build", "Grok Build", false],
    ["grok-2", "Grok 2", true],
  ]);
  assert.deepEqual(catalog.result.models[0].efforts.map((effort) => effort.id), ["low", "high"]);
  assert.equal(catalog.result.models[0].defaultEffort, "low");
});

test("an ACP v2 setup shapes configId options and subject permission requests", async () => {
  const body = [
    request("initialize", "initialize", { protocolVersion: 2, clientInfo: "<any>", clientCapabilities: "<any>" }),
    reply("initialized", "initialize", {
      protocolVersion: 2,
      info: { name: "v2-agent", version: "1" },
      capabilities: { session: { prompt: {}, mcp: {} } },
      authMethods: [],
    }),
    request("session.new", "session/new", { cwd: "<workspace>", mcpServers: "<any>" }),
    reply("session.created", "session/new", {
      sessionId: "s1",
      configOptions: [
        { configId: "model", name: "Model", category: "model", type: "select", currentValue: "m", options: [{ value: "m", name: "M" }] },
      ],
    }),
    promptLine(),
    ask("permission", "session/request_permission", {
      sessionId: "s1",
      title: "Run it",
      subject: { type: "command", toolCallId: "c1" },
      options: [{ optionId: "once", name: "Once", kind: "allow_once" }, { optionId: "reject", name: "No", kind: "reject_once" }],
    }),
    answer("permission.response", "session/request_permission", { outcome: { outcome: "selected", optionId: "once" } }),
    reply("done", "session/prompt", { stopReason: "end_turn" }),
  ];
  const { replay, h } = await syntheticReady(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  const askFrame = await h.next((frame) => frame.type === "permission");
  assert.equal(askFrame.tool, "Run it");
  assert.equal(askFrame.input.toolCall.toolCallId, "c1");
  assert.equal(askFrame.input.toolCall.kind, "execute");
  h.send({ id: 4, type: "respond", requestId: askFrame.requestId, decision: { behavior: "allow" } });
  await h.next((frame) => frame.type === "runDone");
  h.send({ id: 5, type: "models", sessionId: "s" });
  const catalog = await h.next((frame) => frame.id === 5);
  assert.deepEqual(catalog.result.models.map((model) => [model.id, model.isDefault]), [["m", true]]);
  await settle();
  replay.done();
});

test("model selection uses session/set_model when the session advertises a model state", async () => {
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1", models: { currentModelId: "glm", availableModels: [{ modelId: "glm", name: "GLM" }, { modelId: "kimi", name: "Kimi" }] } });
      if (frame.method === "session/set_model") agent.respond(frame.id, {});
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: WORKSPACE, options: { agent: AGENT, model: "kimi" } });
  assert.equal((await h.next((frame) => frame.id === 2)).ok, true);
  assert.deepEqual(agent.state.written.find((frame) => frame.method === "session/set_model").params, {
    sessionId: "s1",
    modelId: "kimi",
  });
  h.send({ id: 3, type: "models", sessionId: "s" });
  assert.deepEqual((await h.next((frame) => frame.id === 3)).result.models.map((model) => [model.id, model.name, model.isDefault]), [
    ["glm", "GLM", false],
    ["kimi", "Kimi", true],
  ]);
});

test("models starts and stops a throwaway agent when no session is open", async () => {
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") {
        agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true }, authMethods: [] });
      }
      if (frame.method === "session/new") {
        agent.respond(frame.id, {
          sessionId: "catalog-session",
          configOptions: [{ id: "model", name: "Model", category: "model", type: "select", currentValue: "glm", options: [{ value: "glm", name: "GLM" }, { value: "kimi", name: "Kimi" }] }],
        });
      }
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  h.send({ id: 2, type: "models", agent: AGENT, cwd: WORKSPACE });
  const catalog = await h.next((frame) => frame.id === 2);
  assert.equal(catalog.ok, true, JSON.stringify(catalog));
  assert.deepEqual(catalog.result.models.map((model) => [model.id, model.isDefault]), [["glm", true], ["kimi", false]]);
  await settle();
  assert.equal(agent.state.closed, 1);
});

test("closeSession ends an open run cancelled, closes the agent and removes the session", async () => {
  let promptId;
  const agent = scripted({
    onRequest(frame) {
      if (frame.method === "initialize") agent.respond(frame.id, { protocolVersion: 1, agentCapabilities: { loadSession: true, sessionCapabilities: { close: {} } }, authMethods: [] });
      if (frame.method === "session/new") agent.respond(frame.id, { sessionId: "s1" });
      if (frame.method === "session/prompt") promptId = frame.id;
      if (frame.method === "session/cancel") agent.respond(promptId, { stopReason: "cancelled" });
      if (frame.method === "session/close") agent.respond(frame.id, {});
    },
  });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  assert.equal((await open(h)).ok, true);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.id === 3);
  h.send({ id: 4, type: "closeSession", sessionId: "s" });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, null);
  const done = await h.next((frame) => frame.type === "runDone");
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.ok(agent.state.written.some((frame) => frame.method === "session/close"), "session/close was advertised and sent");
  assert.equal(agent.state.closed, 1);
  h.send({ id: 5, type: "send", sessionId: "s", message: "again" });
  assert.equal((await h.next((frame) => frame.id === 5)).error.cause.name, "UnknownSession");
});

// ---------------------------------------------------------------------------------------------
// Pure helpers.
// ---------------------------------------------------------------------------------------------

test("acpLaunch resolves PATH and PATHEXT on Windows and leaves other platforms alone", () => {
  const env = { Path: "C:\\bin;C:\\npm", PathExt: ".EXE;.CMD", comspec: "C:\\Windows\\System32\\cmd.exe" };
  const only = (hit) => ({ platform: "win32", exists: (path) => path === hit });
  assert.deepEqual(acpLaunch("agent", env, only("C:\\npm\\agent.CMD")), { command: "C:\\Windows\\System32\\cmd.exe", args: ["/d", "/c", "C:\\npm\\agent.CMD"] });
  assert.deepEqual(acpLaunch("agent", env, only("C:\\bin\\agent.EXE")), { command: "C:\\bin\\agent.EXE", args: [] });
  assert.deepEqual(acpLaunch("C:\\Program Files\\a\\agent.cmd", env, only("C:\\Program Files\\a\\agent.cmd")).args, ["/d", "/c", "@", "C:\\Program Files\\a\\agent.cmd"]);
  assert.deepEqual(acpLaunch("agent", env, only("nothing")), { command: "agent", args: [] });
  assert.deepEqual(acpLaunch("agent", env, { platform: "darwin" }), { command: "agent", args: [] });
});

test("acpModelCatalog shapes models, efforts and defaults from config options and model state", () => {
  const base = {
    configOptions: [
      {
        id: "model",
        name: "Model",
        category: "model",
        type: "select",
        currentValue: "grok-build",
        options: [
          { value: "grok-build", name: "Grok Build" },
          { value: "grok-2", name: "", description: "Second" },
        ],
      },
      {
        id: "thinking",
        name: "Thinking",
        category: "thought_level",
        type: "select",
        currentValue: "high",
        options: [
          { groupId: "g", name: "Levels", options: [{ value: "low", name: "Low" }, { value: "high", name: "High", description: "More" }] },
        ],
      },
      { id: "mode", name: "Mode", category: "mode", type: "select", currentValue: "plan", options: [{ value: "plan", name: "Plan" }] },
    ],
    models: {
      currentModelId: "grok-build",
      availableModels: [
        { modelId: "grok-build", name: "Grok Build" },
        { modelId: "grok-2", name: "Grok 2", _meta: { supportsReasoningEffort: false } },
        { modelId: "grok-3", name: "Grok 3", _meta: { reasoningEfforts: [{ value: "minimal", label: "Minimal" }], reasoningEffort: "minimal" } },
      ],
    },
  };
  const catalog = acpModelCatalog(base);
  assert.deepEqual(catalog.models.map((model) => [model.id, model.name, model.isDefault]), [
    ["grok-build", "Grok Build", true],
    ["grok-2", "grok-2", false],
  ]);
  assert.deepEqual(catalog.models[0].efforts, [
    { id: "low", label: "Low" },
    { id: "high", label: "High", description: "More", isDefault: true },
  ]);
  assert.equal(catalog.models[0].defaultEffort, "high");
  assert.deepEqual(catalog.models[1].efforts, [], "the model says it has no reasoning levels");

  // The Mistral Vibe quirk: its level select is category "thinking", which ACP reserves but
  // never defined (T3: isAcpThoughtLevelOption).
  const quirk = acpModelCatalog({
    configOptions: [
      { id: "model", name: "Model", category: "model", type: "select", currentValue: "m", options: [{ value: "m", name: "M" }] },
      { id: "thinking", name: "Thinking", category: "thinking", type: "select", currentValue: "low", options: [{ value: "low", name: "Low" }] },
    ],
  });
  assert.deepEqual(quirk.models[0].efforts, [{ id: "low", label: "Low", isDefault: true }]);

  const bare = acpModelCatalog({
    models: { currentModelId: "glm", availableModels: [{ modelId: "glm", name: "" }, { modelId: "kimi", name: "Kimi" }] },
  });
  assert.deepEqual(bare.models.map((model) => [model.id, model.name]), [["glm", "glm"], ["kimi", "Kimi"]]);

  // A model's own advertised levels win over the session option.
  const advertised = acpModelCatalog({
    configOptions: [
      { id: "model", name: "Model", category: "model", type: "select", currentValue: "opus", options: [{ value: "opus", name: "Opus" }, { value: "haiku", name: "Haiku" }] },
      { id: "effort", name: "Effort", category: "thought_level", type: "select", currentValue: "high", options: [{ value: "high", name: "High" }] },
    ],
    models: {
      currentModelId: "opus",
      availableModels: [
        { modelId: "opus", name: "Opus" },
        { modelId: "haiku", name: "Haiku", _meta: { reasoningEfforts: [{ value: "low", label: "Low" }], reasoningEffort: "low" } },
      ],
    },
  });
  assert.deepEqual(advertised.models.map((model) => [model.id, model.efforts.map((effort) => effort.id), model.defaultEffort]), [
    ["opus", ["high"], "high"],
    ["haiku", ["low"], "low"],
  ]);
  assert.deepEqual(acpModelCatalog({}).models, []);
});

test("acpStderrExcerpt redacts home paths, bearer tokens and keys", () => {
  const excerpt = acpStderrExcerpt(
    ["Invalid config at /home/server/x.json", "Authorization: Bearer secret-token", "key=sk-abcdefghijklmnop", "Bearer another"].join("\n"),
    "/home/server",
    { HOME: "/home/server" },
  );
  assert.match(excerpt, /~\/x\.json/);
  assert.match(excerpt, /Bearer \[redacted\]/);
  assert.ok(!excerpt.includes("secret-token"));
  assert.ok(!excerpt.includes("sk-abcdefghijklmnop"));
});

// A hand-written agent process for the synthetic cases: `onRequest` sees every line the family
// writes and injects the agent's lines through `send`/`respond`/`notify`.
function scripted({ onRequest } = {}) {
  const state = { spawned: 0, closed: 0, written: [], authenticated: false };
  let line;
  let exit;
  const api = {
    send: (frame) => line(JSON.stringify(frame)),
    respond: (id, result) => api.send({ jsonrpc: "2.0", id, result }),
    fail: (id, error) => api.send({ jsonrpc: "2.0", id, error }),
    notify: (method, params) => api.send({ jsonrpc: "2.0", method, params }),
    request: (id, method, params) => api.send({ jsonrpc: "2.0", id, method, params }),
    exit: (code = 1, signal = null) => exit({ code, signal }),
    spawn: (options) => {
      state.spawned++;
      state.options = options;
      return {
        write(text) {
          const frame = JSON.parse(text);
          state.written.push(frame);
          if (frame.method === "authenticate") state.authenticated = true;
          onRequest?.(frame, api);
        },
        onLine(handler) {
          line = handler;
        },
        onStderr(handler) {
          state.stderrHandler = handler;
        },
        onError() {},
        exited: new Promise((resolve) => (exit = resolve)),
        async close() {
          state.closed++;
          exit({ code: 0, signal: null });
        },
      };
    },
    state,
    stderr: (text) => state.stderrHandler?.(text),
    done: () => {
      assert.deepEqual(state.fatal, undefined);
    },
  };
  return api;
}
