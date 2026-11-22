// Driven by T3 Code codex_transcript.ndjson recordings (MIT, Copyright (c) 2026 T3 Tools Inc.,
// commit 9168ca73d02003557725430b515b44e1551544af; see test/fixtures/NOTICE) and by synthetic
// data shaped like T3's CodexTurnTokenUsage, codexUsageLimits and model/list tests.
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { codexFamily, codexLaunch, codexLimits, codexModels, mapCodexError, SDK_VERSION } from "../src/families/codex.mjs";
import { runHost } from "../src/host.mjs";
import { replayCodex } from "./replayCodex.mjs";

const fixture = (name) => fileURLToPath(new URL(`./fixtures/codex/${name}.ndjson`, import.meta.url));

function boot({ spawn, ...familyOptions }) {
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const all = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    const frame = JSON.parse(line);
    all.push(frame);
    frames.push(frame);
    for (const wait of [...waiters]) wait();
  });
  runHost({ family: codexFamily({ spawn, ...familyOptions }), input, output });
  const next = (pred) =>
    new Promise((resolve) => {
      const check = () => {
        const i = frames.findIndex(pred);
        if (i < 0) return false;
        resolve(frames.splice(i, 1)[0]);
        const w = waiters.indexOf(check);
        if (w >= 0) waiters.splice(w, 1);
        return true;
      };
      if (!check()) waiters.push(check);
    });
  const send = (frame) => input.write(`${JSON.stringify(frame)}\n`);
  return { next, send, all, input };
}

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const runs = (h, sendId) => h.all.filter((frame) => frame.type === "runDone" && frame.sendId === sendId);

async function handshake(h) {
  await h.next((frame) => frame.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((frame) => frame.id === 1);
}

async function open(h, { options = {}, ...extra } = {}, id = 2, sessionId = "s") {
  h.send({ id, type: "open", operation: "create", sessionId, cwd: "/work", options, ...extra });
  const response = await h.next((frame) => frame.id === id);
  assert.equal(response.ok, true, JSON.stringify(response));
  return response;
}

async function ready(name, familyOptions = {}) {
  const replay = replayCodex(fixture(name));
  const h = boot({ spawn: replay.spawn, ...familyOptions });
  await handshake(h);
  const opened = await open(h, { options: { model: replay.model() } });
  return { replay, h, opened };
}

async function send(h, replay, index, id = 3) {
  h.send({ id, type: "send", sessionId: "s", message: replay.promptTexts()[index] });
  const accepted = await h.next((frame) => frame.id === id);
  assert.equal(accepted.ok, true, JSON.stringify(accepted));
  return accepted;
}

function assertAsking(replay) {
  for (const frame of replay.outbound.filter((entry) => entry.method === "turn/start")) {
    assert.equal(frame.params.approvalPolicy, "on-request", "turn/start asks");
    assert.notEqual(frame.params.sandboxPolicy?.type, "dangerFullAccess", "never full access");
    assert.ok(["readOnly", "workspaceWrite"].includes(frame.params.sandboxPolicy?.type), "sandbox is asking");
  }
  for (const frame of replay.outbound.filter((entry) => entry.method === "thread/start" || entry.method === "thread/resume" || entry.method === "thread/fork")) {
    assert.notEqual(frame.params.approvalPolicy, "never");
    assert.notEqual(frame.params.sandbox, "danger-full-access");
  }
}

// A small fake app-server for scenarios the transcripts do not hold. `handler(message, reply)` answers
// each request; `state.methods` is what the family wrote.
function fakeServer(handler) {
  const state = { spawns: [], methods: [], frames: [], children: [] };
  const spawn = (options) => {
    state.spawns.push(options);
    const child = new EventEmitter();
    child.pid = 4242;
    child.killed = false;
    child.stdin = new PassThrough();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    state.children.push(child);
    const reply = (frame) => child.stdout.write(`${JSON.stringify(frame)}\n`);
    let remainder = "";
    child.stdin.setEncoding("utf8");
    child.stdin.on("data", (chunk) => {
      remainder += chunk;
      let newline;
      while ((newline = remainder.indexOf("\n")) >= 0) {
        const line = remainder.slice(0, newline).replace(/\r$/, "");
        remainder = remainder.slice(newline + 1);
        if (line.trim() === "") continue;
        const message = JSON.parse(line);
        state.methods.push(message.method ?? "reply");
        state.frames.push(message);
        handler(message, reply, state);
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
  };
  return { spawn, state };
}

function basicEngine(message, reply, state) {
  state.turn ??= 0;
  if (message.method === "initialize") return reply({ id: message.id, result: { userAgent: "codex_cli/app-server/0.156.1", codexHome: "/home/u/.codex", platformFamily: "unix", platformOs: "linux" } });
  if (message.method === "thread/start") return reply({ id: message.id, result: { thread: { id: "thread-1" }, approvalPolicy: "on-request", sandbox: { type: "workspaceWrite" } } });
  if (message.method === "thread/resume") return reply({ id: message.id, result: { thread: { id: message.params.threadId }, approvalPolicy: "on-request" } });
  if (message.method === "thread/fork") return reply({ id: message.id, result: { thread: { id: "thread-fork" }, approvalPolicy: "on-request" } });
  if (message.method === "turn/start") {
    const threadId = message.params.threadId;
    const turnId = `turn-${state.turn++}`;
    reply({ id: message.id, result: { turn: { id: turnId } } });
    queueMicrotask(() => {
      reply({ method: "turn/started", params: { threadId, turn: { id: turnId } } });
      reply({ method: "turn/completed", params: { threadId, turn: { id: turnId, status: "completed", error: null } } });
    });
    return;
  }
  if (message.method === "turn/interrupt") return reply({ id: message.id, result: {} });
  if (message.method === "thread/read") return reply({ id: message.id, result: { thread: { id: message.params.threadId, turns: [{ id: "turn-1" }] } } });
  return reply({ id: message.id, result: {} });
}

const engine = (overrides = {}) => (message, reply, state) => {
  const handler = overrides[message.method];
  if (handler) return handler(message, reply, state);
  return basicEngine(message, reply, state);
};

test("hello names the codex family, the pinned app-server version and the requests it implements", async () => {
  const fake = fakeServer(basicEngine);
  const h = boot({ spawn: fake.spawn });
  const hello = await h.next((frame) => frame.type === "hello");
  assert.equal(hello.family, "codex");
  assert.equal(hello.sdk.name, "codex app-server");
  assert.equal(hello.sdk.version, SDK_VERSION);
  assert.equal(SDK_VERSION, "0.156.1");
  assert.deepEqual(hello.capabilities, ["steer", "fork", "rollback", "models", "usage", "readHistory"]);
});

test("simple: thread/start, one turn, deltas, usage and exactly one runDone", async () => {
  const { replay, h, opened } = await ready("simple");
  assert.equal(opened.result.engineSessionId, replay.sessionId());
  const accepted = await send(h, replay, 0);
  assert.equal(accepted.result.engineSessionId, replay.sessionId());
  assert.ok(accepted.result.turnId);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "completed");
  assert.equal(done.result.threadDisposition, "reusable");
  assert.ok(done.result.usage.inputTokens > 0);
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.sendId === 3 && frame.update.method === "item/agentMessage/delta"));
  assertAsking(replay);
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("multi_turn: two sends on one thread, one runDone each", async () => {
  const { replay, h } = await ready("multi_turn");
  await send(h, replay, 0, 3);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  await send(h, replay, 1, 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 4)).result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.equal(runs(h, 4).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("message_steering: steer reaches turn/steer and is accepted", async () => {
  const { replay, h } = await ready("message_steering");
  await send(h, replay, 0);
  h.send({ id: 4, type: "steer", sendId: 3, messageId: "m1", text: "Actually, respond with exactly: steering fixture observed" });
  const steered = await h.next((frame) => frame.id === 4);
  assert.deepEqual(steered.result, { outcome: "accepted", messageId: "m1" });
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("queued_turn: a second send starts after the first run ends", async () => {
  const { replay, h } = await ready("queued_turn");
  await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  await send(h, replay, 1, 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 4)).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("turn_interrupt: cancel sends turn/interrupt and ends the run cancelled, once", async () => {
  const { replay, h } = await ready("turn_interrupt");
  await send(h, replay, 0);
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("turn_interrupt_mid_tool: cancel mid command streams deltas and ends cancelled", async () => {
  const { replay, h } = await ready("turn_interrupt_mid_tool");
  await send(h, replay, 0);
  await h.next((frame) => frame.type === "delta" && frame.update.method === "item/started" && frame.update.params?.item?.type === "commandExecution");
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { cancelled: true });
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "cancelled");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("tool_call_read_only_on_request: the ask is a permission frame and allow is once-only", async () => {
  const { replay, h } = await ready("tool_call_read_only_on_request");
  await send(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission" && frame.sendId === 3);
  assert.equal(ask.tool, "command");
  assert.match(ask.input.command, /codex-probe-write-action/);
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, null);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  const replies = replay.outbound.filter((frame) => frame.method === undefined && frame.result);
  assert.deepEqual(replies.map((frame) => frame.result.decision), ["accept"]);
  assert.ok(!JSON.stringify(replies).includes("acceptForSession"));
  assertAsking(replay);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("tool_call_restricted_granular: deny answers decline and the turn continues", async () => {
  const { replay, h } = await ready("tool_call_restricted_granular");
  await send(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission");
  assert.equal(ask.tool, "fileChange");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "deny", message: "no" } });
  await h.next((frame) => frame.id === 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  const reply = replay.outbound.find((frame) => frame.method === undefined && frame.result);
  assert.equal(reply.result.decision, "decline");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("tool_call_workspace_never: the recorded never/full-access values are replaced by asking one", async () => {
  const { replay, h } = await ready("tool_call_workspace_never");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assertAsking(replay);
  assert.ok(!JSON.stringify(replay.outbound).includes('"never"'));
  assert.ok(!JSON.stringify(replay.outbound).includes("dangerFullAccess"));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("command_output_streaming: command output notifications pass through as deltas", async () => {
  const { replay, h } = await ready("command_output_streaming");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.method === "item/commandExecution/outputDelta"));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("plan_questions: requestUserInput is a question frame answered by question text", async () => {
  const { replay, h } = await ready("plan_questions");
  await send(h, replay, 0);
  const question = await h.next((frame) => frame.type === "question" && frame.sendId === 3);
  assert.equal(question.tool, "request_user_input");
  assert.equal(question.input.questions[0].id, "schema_preference");
  h.send({ id: 4, type: "respond", requestId: question.requestId, decision: { answers: { "Should this fixture prefer strict schemas or UI flexibility?": "Strict schemas (Recommended)" } } });
  assert.equal((await h.next((frame) => frame.id === 4)).ok, true);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  const reply = replay.outbound.find((frame) => frame.method === undefined && frame.result);
  assert.deepEqual(reply.result.answers, { schema_preference: { answers: ["Strict schemas (Recommended)"] } });
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("proposed_plan: plan deltas pass through and the run completes", async () => {
  const { replay, h } = await ready("proposed_plan");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.method === "item/plan/delta"));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("todo_list: turn/plan/updated passes through and the run completes", async () => {
  const { replay, h } = await ready("todo_list");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.method === "turn/plan/updated"));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("web_search: a webSearch item passes through untouched", async () => {
  const { replay, h } = await ready("web_search");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  const searched = h.all.find((frame) => frame.type === "delta" && frame.update.method === "item/completed" && frame.update.params?.item?.type === "webSearch");
  assert.ok(searched, "webSearch item/completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("provider_thread_resume: close the process, resume by thread id in a new app-server", async () => {
  const { replay, h } = await ready("provider_thread_resume");
  const sessionId = replay.sessionId();
  await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  h.send({ id: 4, type: "closeSession", sessionId: "s" });
  assert.equal((await h.next((frame) => frame.id === 4)).ok, true);
  await settle();
  assert.equal(replay.closed(), true, "the first app-server was shut down");
  h.send({ id: 5, type: "open", operation: "resume", sessionId: "s", engineSessionId: sessionId, cwd: "/work", options: { model: replay.model() } });
  const resumed = await h.next((frame) => frame.id === 5);
  assert.equal(resumed.ok, true, JSON.stringify(resumed));
  assert.deepEqual(resumed.result, { engineSessionId: sessionId });
  await send(h, replay, 1, 6);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 6)).result.reason, "completed");
  assert.equal(replay.spawns.length, 2);
  assert.ok(replay.methods().includes("thread/resume"));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("thread_fork_native_prior_turn: fork at the earlier turn through thread/fork, then start a turn on the fork", async () => {
  const { replay, h } = await ready("thread_fork_native_prior_turn");
  const first = await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  await send(h, replay, 1, 4);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 4);
  h.send({ id: 5, type: "open", operation: "fork", sessionId: "f", engineSessionId: replay.sessionId(), cwd: "/work", options: { model: replay.model(), atTurn: first.result.turnId } });
  const forked = await h.next((frame) => frame.id === 5);
  assert.equal(forked.ok, true, JSON.stringify(forked));
  assert.ok(forked.result.engineSessionId);
  h.send({ id: 6, type: "send", sessionId: "f", message: replay.promptTexts()[2] });
  assert.equal((await h.next((frame) => frame.id === 6)).ok, true);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 6)).result.reason, "completed");
  assert.ok(replay.outbound.some((frame) => frame.method === "thread/fork" && frame.params.lastTurnId === first.result.turnId));
  assert.equal(replay.spawns.length, 1, "the fork stays in the same app-server");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("thread_rollback: rollback keeps toTurn, reverts later turns, then the thread continues", async () => {
  const { replay, h } = await ready("thread_rollback");
  const first = await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  const second = await send(h, replay, 1, 4);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 4);
  // toTurn is kept; the recording reverts the one turn after it.
  h.send({ id: 5, type: "rollback", sessionId: "s", toTurn: first.result.turnId });
  assert.deepEqual((await h.next((frame) => frame.id === 5)).result, { ok: true });
  await send(h, replay, 2, 6);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 6)).result.reason, "completed");
  assert.ok(replay.outbound.some((frame) => frame.method === "thread/revert" && frame.params.beforeTurnId === second.result.turnId));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("thread_rollback_after_restart: resume, load on demand, revert, reload, then continue", async () => {
  const { replay, h } = await ready("thread_rollback_after_restart");
  const sessionId = replay.sessionId();
  const first = await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  const second = await send(h, replay, 1, 4);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 4);
  h.send({ id: 5, type: "closeSession", sessionId: "s" });
  await h.next((frame) => frame.id === 5);
  await settle();
  h.send({ id: 6, type: "open", operation: "resume", sessionId: "s", engineSessionId: sessionId, cwd: "/work", options: { model: replay.model() } });
  assert.equal((await h.next((frame) => frame.id === 6)).ok, true);
  h.send({ id: 7, type: "rollback", sessionId: "s", toTurn: first.result.turnId });
  assert.deepEqual((await h.next((frame) => frame.id === 7)).result, { ok: true });
  await send(h, replay, 2, 8);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 8)).result.reason, "completed");
  assert.ok(replay.outbound.some((frame) => frame.method === "thread/revert" && frame.params.beforeTurnId === second.result.turnId));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("thread_rollback_to_stopped_turn: an interrupted turn is a rollback target", async () => {
  const { replay, h } = await ready("thread_rollback_to_stopped_turn");
  await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  const stopped = await send(h, replay, 1, 4);
  h.send({ id: 5, type: "cancel", sendId: 4 });
  assert.deepEqual((await h.next((frame) => frame.id === 5)).result, { cancelled: true });
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 4)).result.reason, "cancelled");
  const after = await send(h, replay, 2, 6);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 6);
  // toTurn is kept: rolling back to the stopped turn reverts the completed turn after it.
  h.send({ id: 7, type: "rollback", sessionId: "s", toTurn: stopped.result.turnId });
  assert.deepEqual((await h.next((frame) => frame.id === 7)).result, { ok: true });
  await send(h, replay, 3, 8);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 8)).result.reason, "completed");
  assert.ok(replay.outbound.some((frame) => frame.method === "thread/revert" && frame.params.beforeTurnId === after.result.turnId));
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent: child threads are background frames and one runDone follows the parent's turn", async () => {
  const { replay, h } = await ready("subagent");
  await send(h, replay, 0);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.result.reason, "completed");
  const started = h.all.filter((frame) => frame.type === "background" && frame.event.type === "started");
  const ended = h.all.filter((frame) => frame.type === "background" && frame.event.type === "ended");
  assert.ok(started.length > 0, "background started");
  assert.ok(ended.length > 0, "background ended");
  assert.ok(started.every((frame) => frame.event.kind === "agent"));
  assert.ok(started.every((frame) => typeof frame.event.id === "string" && frame.event.id.length > 0));
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent_continue: a second root turn resumes the same child and each turn ends once", async () => {
  const { replay, h } = await ready("subagent_continue");
  await send(h, replay, 0, 3);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  await send(h, replay, 1, 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 4)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "background" && frame.event.type === "started"));
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.equal(runs(h, 4).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent_v2: multi-agent v2 activity is reported and the run ends once", async () => {
  const { replay, h } = await ready("subagent_v2");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "background" && frame.event.type === "started" && frame.event.kind === "agent"));
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent_v2_approval: a subagent's approval reaches the run as a permission frame", async () => {
  const { replay, h } = await ready("subagent_v2_approval");
  await send(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission" && frame.sendId === 3);
  assert.equal(ask.tool, "command");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  assert.equal((await h.next((frame) => frame.id === 4)).ok, true);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent_v2_nested: a chain of agents is reported and the run ends once", async () => {
  const { replay, h } = await ready("subagent_v2_nested");
  await send(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  const ids = new Set(h.all.filter((frame) => frame.type === "background" && frame.event.type === "started").map((frame) => frame.event.id));
  assert.ok(ids.size >= 3, "three agents reported");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent_v2_nested_approval: a nested agent's approval is answered on the parent run", async () => {
  const { replay, h } = await ready("subagent_v2_nested_approval");
  await send(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission" && frame.sendId === 3);
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  await h.next((frame) => frame.id === 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("thread_merge_back_siblings: forks and the source share one app-server and each turn ends once", async () => {
  const { replay, h } = await ready("thread_merge_back_siblings");
  const first = await send(h, replay, 0, 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  for (const [id, session, prompt, sendId] of [[4, "f1", 1, 5], [6, "f2", 2, 7]]) {
    h.send({ id, type: "open", operation: "fork", sessionId: session, engineSessionId: replay.sessionId(), cwd: "/work", options: { model: replay.model(), atTurn: first.result.turnId } });
    assert.equal((await h.next((frame) => frame.id === id)).ok, true);
    h.send({ id: sendId, type: "send", sessionId: session, message: replay.promptTexts()[prompt] });
    assert.equal((await h.next((frame) => frame.id === sendId)).ok, true);
    assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === sendId)).result.reason, "completed");
  }
  for (const [index, id] of [3, 4, 5].entries()) {
    h.send({ id: 8 + index, type: "send", sessionId: "s", message: replay.promptTexts()[index + 3] });
    assert.equal((await h.next((frame) => frame.id === 8 + index)).ok, true);
    assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 8 + index)).result.reason, "completed");
  }
  assert.equal(replay.spawns.length, 1, "all threads stay in one app-server");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("delegated_task_status: another thread's turns never end the parent run", async () => {
  const { replay, h } = await ready("delegated_task_status");
  await send(h, replay, 0);
  await settle(50);
  assert.equal(runs(h, 3).length, 0, "the parent turn is still open");
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.method === "turn/completed"));
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
});

test("codex_model_reroute: model/rerouted passes through and the next turn still completes", async () => {
  const { replay, h } = await ready("codex_model_reroute");
  await send(h, replay, 0, 3);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 3)).result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.method === "model/rerouted"));
  await send(h, replay, 1, 4);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === 4)).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("Windows launcher: PATH and PATHEXT resolve codex, a .cmd runs under cmd.exe, other platforms are unchanged", () => {
  const env = { Path: "C:\\bin;C:\\npm", PathExt: ".EXE;.CMD", comspec: "C:\\Windows\\System32\\cmd.exe" };
  const only = (hit) => ({ platform: "win32", exists: (path) => path === hit });
  assert.deepEqual(codexLaunch("codex", env, only("C:\\npm\\codex.CMD")), { command: "C:\\Windows\\System32\\cmd.exe", args: ["/d", "/c", "C:\\npm\\codex.CMD"] });
  assert.deepEqual(codexLaunch("codex", env, only("C:\\bin\\codex.EXE")), { command: "C:\\bin\\codex.EXE", args: [] });
  assert.deepEqual(codexLaunch("C:\\Program Files\\OpenAI\\codex.cmd", env, only("C:\\Program Files\\OpenAI\\codex.cmd")).args, ["/d", "/c", "@", "C:\\Program Files\\OpenAI\\codex.cmd"]);
  assert.deepEqual(codexLaunch("codex", env, only("nothing")), { command: "codex", args: [] }, "unresolved: let spawn report it");
  assert.deepEqual(codexLaunch("codex", env, { platform: "darwin" }), { command: "codex", args: [] });
});

test("confinement: argvPrefix prefixes the app-server command and env is the whole environment", async () => {
  const replay = replayCodex(fixture("simple"));
  const h = boot({ spawn: replay.spawn });
  await handshake(h);
  const confinement = { argvPrefix: ["/usr/bin/sandbox-exec", "-f", "/p.sb", "--"], env: { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1" }, cwd: "/work", writeRoots: ["/work"] };
  const opened = await open(h, { options: { model: replay.model(), codexPath: "/opt/codex", env: { ORBYN_TEST_ADDED: "1" } }, confinement });
  assert.equal(opened.ok, true);
  const [spawn] = replay.spawns;
  assert.equal(spawn.command, "/usr/bin/sandbox-exec");
  assert.deepEqual(spawn.args, ["-f", "/p.sb", "--", "/opt/codex", "app-server", "-c", "thread_unload_delay_secs=0"]);
  assert.deepEqual(spawn.env, { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1", ORBYN_TEST_ADDED: "1" });
  assert.equal(spawn.cwd, "/work");
  const added = h.all.find((frame) => frame.type === "log" && frame.event.type === "envAdded");
  assert.deepEqual(added.event.keys, ["ORBYN_TEST_ADDED"]);
  assert.ok(!JSON.stringify(h.all).includes("ORBYN_WORKER_ID_VALUE"), "no environment values are logged");
});

test("without confinement the host environment is used; options.env additions are logged", async () => {
  process.env.ORBYN_CODEX_TEST = "host";
  const replay = replayCodex(fixture("simple"));
  const h = boot({ spawn: replay.spawn });
  await handshake(h);
  await open(h, { options: { model: replay.model(), env: { ORBYN_TEST_ADDED: "1" } } });
  const [spawn] = replay.spawns;
  assert.equal(spawn.command, "codex");
  assert.equal(spawn.env.ORBYN_CODEX_TEST, "host");
  assert.equal(spawn.env.ORBYN_TEST_ADDED, "1");
  delete process.env.ORBYN_CODEX_TEST;
});

test("open refusals start nothing: bad prefix, bad operation, resume/fork without an engine session", async () => {
  const replay = replayCodex(fixture("simple"));
  const h = boot({ spawn: replay.spawn });
  await handshake(h);
  const refuse = async (id, request, options = {}) => {
    h.send({ id, type: "open", operation: "create", sessionId: `s${id}`, cwd: "/work", options, ...request });
    return h.next((frame) => frame.id === id);
  };
  const badPrefix = await refuse(2, { confinement: { argvPrefix: ["ok", ""] } });
  assert.equal(badPrefix.error.cause.kind, "invalid_request");
  const badMode = await refuse(3, {}, { mode: "full-access" });
  assert.equal(badMode.error.cause.kind, "invalid_request");
  const noResume = await refuse(4, { operation: "resume" });
  assert.equal(noResume.error.cause.kind, "invalid_request");
  const noFork = await refuse(5, { operation: "fork" });
  assert.equal(noFork.error.cause.kind, "invalid_request");
  const unknown = await refuse(6, { operation: "branch" });
  assert.equal(unknown.error.cause.kind, "unsupported");
  const never = await refuse(7, {}, { approvalPolicy: "never" });
  assert.equal(never.error.cause.kind, "invalid_request");
  assert.equal(replay.spawns.length, 0);
});

test("a late respond is ignored and logged, never sent to the engine", async () => {
  const { replay, h } = await ready("simple");
  await send(h, replay, 0);
  h.send({ id: 4, type: "respond", requestId: "p-99", decision: { behavior: "allow" } });
  assert.equal((await h.next((frame) => frame.id === 4)).ok, true);
  assert.ok(h.all.some((frame) => frame.type === "log" && frame.event.type === "lateRespond"));
  assert.ok(!replay.outbound.some((frame) => frame.method === undefined));
});

test("cancel refuses a pending approval and ends the run cancelled", async () => {
  let turn = 0;
  const decisions = [];
  const fake = fakeServer((message, reply, state) => {
    if (message.method === "initialize") return reply({ id: message.id, result: { userAgent: "codex_cli/app-server/0.156.1" } });
    if (message.method === "thread/start") return reply({ id: message.id, result: { thread: { id: "thread-1" } } });
    if (message.method === "turn/start") {
      const turnId = `turn-${turn++}`;
      reply({ id: message.id, result: { turn: { id: turnId } } });
      queueMicrotask(() => reply({ method: "item/commandExecution/requestApproval", id: 0, params: { threadId: "thread-1", turnId, itemId: "item-1", command: "touch x", availableDecisions: ["accept", "decline", "cancel"] } }));
      return;
    }
    if (message.method === "turn/interrupt") {
      reply({ id: message.id, result: {} });
      queueMicrotask(() => reply({ method: "turn/completed", params: { threadId: "thread-1", turn: { id: "turn-0", status: "interrupted", error: null } } }));
      return;
    }
    if (message.method === undefined) {
      decisions.push(message.result?.decision);
      return;
    }
    return reply({ id: message.id, result: {} });
  });
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  assert.equal((await h.next((frame) => frame.id === 3)).ok, true);
  await h.next((frame) => frame.type === "permission");
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(decisions, ["cancel"]);
});

test("models lists model/list with efforts from a throwaway process that is closed again", async () => {
  const fake = fakeServer(engine({
    "model/list": (message, reply) => reply({ id: message.id, result: { data: [
      { model: "gpt-6-luna", displayName: "GPT-6 Luna", isDefault: true, supportedReasoningEfforts: [{ reasoningEffort: "low" }, { reasoningEffort: "high" }], defaultReasoningEffort: "high" },
      { model: "gpt-6-sol", displayName: "GPT-6 Sol", isDefault: false, supportedReasoningEfforts: [{ reasoningEffort: "medium" }], defaultReasoningEffort: "medium" },
    ], nextCursor: null } }),
    "account/read": (message, reply) => reply({ id: message.id, result: { account: { type: "chatgpt", planType: "plus", email: "a@example.test" }, requiresOpenaiAuth: true } }),
  }));
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  h.send({ id: 5, type: "models", cwd: "/work" });
  const response = await h.next((frame) => frame.id === 5);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.deepEqual(response.result.models.map((model) => [model.id, model.name, model.isDefault, model.defaultEffort]), [
    ["gpt-6-luna", "GPT-6 Luna", true, "high"],
    ["gpt-6-sol", "GPT-6 Sol", false, "medium"],
  ]);
  assert.deepEqual(response.result.models[0].efforts, [
    { id: "low", label: "Low" },
    { id: "high", label: "High", isDefault: true },
  ]);
  assert.deepEqual(response.result.account, { type: "chatgpt", planType: "plus", email: "a@example.test" });
  assert.equal(fake.state.children[0].killed, true, "the throwaway process was closed");
});

test("usage maps account/rateLimits/read to windows and a plan label", async () => {
  const fake = fakeServer(engine({
    "account/rateLimits/read": (message, reply) => reply({ id: message.id, result: {
      rateLimits: { planType: "plus", primary: { usedPercent: 12.5, resetsAt: 1_800_000_000, windowDurationMins: 300 }, secondary: { usedPercent: 60, resetsAt: 1_800_600_000 } },
      rateLimitsByLimitId: null,
    } }),
  }));
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  h.send({ id: 5, type: "usage" });
  const response = await h.next((frame) => frame.id === 5);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.equal(response.result.usage, null);
  assert.ok(response.result.limits.checkedAt);
  assert.deepEqual(response.result.limits.windows.map((window) => [window.id, window.kind, window.usedPercent]), [
    ["primary", "session", 12.5],
    ["secondary", "weekly", 60],
  ]);
  assert.equal(response.result.limits.authLabel, "ChatGPT Plus Subscription");
  assert.equal(fake.state.children[0].killed, true);
});

test("error mapping covers auth, rate limits, network, invalid requests, unsupported and lost sessions", () => {
  const kind = (cause) => mapCodexError(cause).kind;
  assert.equal(kind(Object.assign(new Error("not logged in"), { name: "CodexRequestError", code: -32000 })), "auth");
  assert.equal(kind(Object.assign(new Error("429 usage limit reached"), { name: "CodexRequestError" })), "rate_limit");
  assert.equal(kind(Object.assign(new Error("fetch failed: ECONNRESET"), { name: "Error", code: "ECONNRESET" })), "network");
  assert.equal(kind(Object.assign(new Error("invalid params for turn/start"), { name: "CodexRequestError", code: -32602 })), "invalid_request");
  assert.equal(kind(Object.assign(new Error("Method not found"), { name: "CodexRequestError", code: -32601 })), "unsupported");
  assert.equal(kind(new Error("thread not found")), "session_lost");
  assert.equal(kind(Object.assign(new Error("Server overloaded; retry later."), { name: "CodexRequestError", code: -32001 })), "network");
  const managed = mapCodexError(new Error("subscription_sharing_usage_limit_exceeded"));
  assert.equal(managed.kind, "rate_limit");
  assert.match(managed.message, /usage limit/);
  assert.equal(kind(new Error("something else")), "engine");
});

test("resume of a thread the engine no longer has is session_lost", async () => {
  const fake = fakeServer(engine({
    "thread/resume": (message, reply) => reply({ id: message.id, error: { code: -32004, message: "thread not found" } }),
  }));
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "resume", sessionId: "s", engineSessionId: "gone", cwd: "/work", options: { model: "gpt-6-luna" } });
  const response = await h.next((frame) => frame.id === 2);
  assert.equal(response.ok, false);
  assert.equal(response.error.cause.kind, "session_lost");
  assert.equal(fake.state.children[0].killed, true, "the failed open stopped its process");
});

test("attachments by path travel as localImage/localAudio input", async () => {
  const fake = fakeServer(engine());
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: { text: "look", attachments: [{ path: "/tmp/a.png", mimeType: "image/png" }, { path: "/tmp/b.wav", mimeType: "audio/wav" }] } });
  assert.equal((await h.next((frame) => frame.id === 3)).ok, true);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  const turn = fake.state.frames.find((frame) => frame.method === "turn/start");
  assert.deepEqual(turn.params.input, [
    { type: "text", text: "look" },
    { type: "localImage", path: "/tmp/a.png" },
    { type: "localAudio", path: "/tmp/b.wav" },
  ]);
});

test("readHistory returns the engine's thread turns untouched", async () => {
  const fake = fakeServer(engine({
    "thread/read": (message, reply) => reply({ id: message.id, result: { thread: { id: message.params.threadId, turns: [{ id: "turn-1", status: "completed" }] } } }),
  }));
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const response = await h.next((frame) => frame.id === 3);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.deepEqual(response.result, { turns: [{ id: "turn-1", status: "completed" }] });
});

test("the fork request sends thread/fork with lastTurnId and returns the new thread id", async () => {
  const fake = fakeServer(engine());
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "fork", sessionId: "s", atTurn: "turn-0" });
  const response = await h.next((frame) => frame.id === 3);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.deepEqual(response.result, { engineSessionId: "thread-fork" });
  const fork = fake.state.frames.find((frame) => frame.method === "thread/fork");
  assert.equal(fork.params.threadId, "thread-1");
  assert.equal(fork.params.lastTurnId, "turn-0");
});

test("a process exit mid-run fails the run once, marks the thread broken and refuses the next send", async () => {
  let child;
  const fake = fakeServer((message, reply, state) => {
    child = state.children.at(-1);
    if (message.method === "turn/start") {
      reply({ id: message.id, result: { turn: { id: "turn-1" } } });
      return;
    }
    return basicEngine(message, reply, state);
  });
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.id === 3);
  child.stdout.end();
  child.emit("exit", 1, null);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.ok, false);
  assert.equal(done.result.threadDisposition, "broken");
  assert.equal(done.error.cause.kind, "engine");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  h.send({ id: 4, type: "send", sessionId: "s", message: "again" });
  assert.equal((await h.next((frame) => frame.id === 4)).error.cause.kind, "session_lost");
});

test("codexModels and codexLimits guard their input", () => {
  assert.deepEqual(codexModels({ data: [] }).models, []);
  assert.equal(codexLimits({ rateLimits: { primary: null } }).windows.length, 0);
  assert.equal(codexLimits({ rateLimits: { limitId: "codex-other", primary: { usedPercent: 3 } } }).windows.length, 0);
});

test("a subagent still running when the parent turn completes holds the run until it ends", async () => {
  const fake = fakeServer((message, reply, state) => {
    state.turn ??= 0;
    if (message.method === "initialize") return reply({ id: message.id, result: { userAgent: "codex_cli/app-server/0.156.1" } });
    if (message.method === "thread/start") return reply({ id: message.id, result: { thread: { id: "thread-root" } } });
    if (message.method === "turn/start") {
      const turnId = `turn-${state.turn++}`;
      reply({ id: message.id, result: { turn: { id: turnId } } });
      queueMicrotask(() => {
        reply({ method: "turn/started", params: { threadId: "thread-root", turn: { id: turnId } } });
        reply({ method: "thread/status/changed", params: { threadId: "thread-child", status: { type: "active", activeFlags: [] } } });
        reply({ method: "turn/completed", params: { threadId: "thread-root", turn: { id: turnId, status: "completed", error: null } } });
      });
      setTimeout(() => reply({ method: "thread/status/changed", params: { threadId: "thread-child", status: { type: "idle" } } }), 20);
      return;
    }
    if (message.method === "turn/interrupt") return reply({ id: message.id, result: {} });
    return reply({ id: message.id, result: {} });
  });
  const h = boot({ spawn: fake.spawn, wakeGraceMs: 10 });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  assert.equal((await h.next((frame) => frame.id === 3)).ok, true);
  await h.next((frame) => frame.type === "background" && frame.event.type === "started");
  await settle(25);
  assert.equal(runs(h, 3).length, 0, "the run is held while the agent runs");
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.result.reason, "completed");
  assert.ok(h.all.some((frame) => frame.type === "background" && frame.event.type === "ended"));
  await settle();
  assert.equal(runs(h, 3).length, 1);
});

test("options.mcpServers travels as mcp_servers.* thread config, and developerInstructions is passed", async () => {
  const fake = fakeServer(engine());
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna", developerInstructions: "Be brief.", mcpServers: { orbyn: { url: "http://127.0.0.1:1/mcp", tool_timeout_sec: 30 } } } });
  const start = fake.state.frames.find((frame) => frame.method === "thread/start");
  assert.equal(start.params.developerInstructions, "Be brief.");
  assert.deepEqual(start.params.config, { "mcp_servers.orbyn": { url: "http://127.0.0.1:1/mcp", tool_timeout_sec: 30 } });
});

test("a turn failure maps codexErrorInfo to a kind and reads the reset time from its text", async () => {
  const fake = fakeServer((message, reply, state) => {
    if (message.method === "initialize") return reply({ id: message.id, result: { userAgent: "codex_cli/app-server/0.156.1" } });
    if (message.method === "thread/start") return reply({ id: message.id, result: { thread: { id: "thread-1" } } });
    if (message.method === "turn/start") {
      reply({ id: message.id, result: { turn: { id: "turn-1" } } });
      queueMicrotask(() => reply({ method: "turn/completed", params: { threadId: "thread-1", turn: { id: "turn-1", status: "failed", error: { message: "Usage limit reached. Try again at 11:59 PM.", codexErrorInfo: { usageLimitExceeded: {} } } } } }));
      return;
    }
    return reply({ id: message.id, result: {} });
  });
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.id === 3);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.equal(done.ok, false);
  assert.equal(done.result.reason, "failed");
  assert.equal(done.error.cause.kind, "rate_limit");
  assert.equal(done.result.retry.why, "rate_limit");
  assert.ok(typeof done.result.retry.resetsAt === "number");
});

test("the default spawn runs a stand-in app-server over real stdio", async () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-codex-"));
  const script = join(dir, "fake-app-server.mjs");
  const wrapper = join(dir, process.platform === "win32" ? "codex.cmd" : "codex");
  writeFileSync(
    script,
    `import { createInterface } from "node:readline";
const out = (frame) => process.stdout.write(JSON.stringify(frame) + "\\n");
createInterface({ input: process.stdin }).on("line", (line) => {
  if (line.trim() === "") return;
  const message = JSON.parse(line);
  if (message.method === "initialize") return out({ id: message.id, result: { userAgent: "codex_cli/app-server/0.156.1", codexHome: "/home/u/.codex", platformFamily: "unix", platformOs: "linux" } });
  if (message.method === "thread/start") return out({ id: message.id, result: { thread: { id: "real-thread" } } });
  if (message.method === "turn/start") {
    out({ id: message.id, result: { turn: { id: "real-turn" } } });
    queueMicrotask(() => {
      out({ method: "turn/started", params: { threadId: "real-thread", turn: { id: "real-turn" } } });
      out({ method: "item/agentMessage/delta", params: { threadId: "real-thread", turnId: "real-turn", itemId: "item-1", delta: "hello" } });
      out({ method: "turn/completed", params: { threadId: "real-thread", turn: { id: "real-turn", status: "completed", error: null } } });
    });
    return;
  }
  if (message.id !== undefined) out({ id: message.id, result: {} });
});
`,
  );
  const quotedNode = `"${process.execPath}"`;
  const quotedScript = `"${script}"`;
  writeFileSync(wrapper, process.platform === "win32" ? `@${quotedNode} ${quotedScript} %*\r\n` : `#!/bin/sh\nexec ${quotedNode} ${quotedScript} "$@"\n`);
  if (process.platform !== "win32") chmodSync(wrapper, 0o755);
  try {
    const h = boot({});
    await handshake(h);
    await open(h, { cwd: dir, options: { codexPath: wrapper, model: "gpt-6-luna" } });
    h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
    assert.equal((await h.next((frame) => frame.id === 3)).ok, true);
    const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
    assert.equal(done.result.reason, "completed");
    assert.ok(h.all.some((frame) => frame.type === "delta" && frame.update.params?.delta === "hello"));
    h.send({ id: 4, type: "close" });
    await h.next((frame) => frame.id === 4);
  } finally {
    rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test("models uses its own throwaway process even when the confinement is already open", async () => {
  const fake = fakeServer(engine({
    "model/list": (message, reply) => reply({ id: message.id, result: { data: [{ model: "gpt-6-luna", displayName: "GPT-6 Luna", supportedReasoningEfforts: [{ reasoningEffort: "high" }], defaultReasoningEffort: "high" }], nextCursor: null } }),
  }));
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna" } });
  const sessionChild = fake.state.children[0];
  h.send({ id: 5, type: "models", cwd: "/work" });
  assert.equal((await h.next((frame) => frame.id === 5)).ok, true);
  assert.equal(fake.state.children.length, 2);
  assert.equal(fake.state.children[1].killed, true, "the throwaway process is closed");
  assert.equal(sessionChild.killed, false, "the session process is not");
  h.send({ id: 6, type: "send", sessionId: "s", message: "hi" });
  assert.equal((await h.next((frame) => frame.id === 6)).ok, true);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 6);
});

test("a missing codex binary is a plain engine error", async () => {
  const spawn = () => {
    const child = new EventEmitter();
    child.stdin = new PassThrough();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.kill = () => {
      child.stdout.end();
      child.emit("exit", 1, null);
      return true;
    };
    queueMicrotask(() => child.emit("error", Object.assign(new Error("spawn codex ENOENT"), { code: "ENOENT" })));
    return child;
  };
  const h = boot({ spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/work", options: {} });
  const response = await h.next((frame) => frame.id === 2);
  assert.equal(response.ok, false);
  assert.equal(response.error.cause.kind, "engine");
  assert.match(response.error.cause.message, /not installed/);
});

test("workspace-write mode and the stricter asking policy reach turn/start", async () => {
  const fake = fakeServer(engine());
  const h = boot({ spawn: fake.spawn });
  await handshake(h);
  await open(h, { options: { model: "gpt-6-luna", mode: "workspace-write", approvalPolicy: "untrusted" } });
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((frame) => frame.id === 3);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  const turn = fake.state.frames.find((frame) => frame.method === "turn/start");
  assert.equal(turn.params.approvalPolicy, "untrusted");
  assert.deepEqual(turn.params.sandboxPolicy, { type: "workspaceWrite" });
});
