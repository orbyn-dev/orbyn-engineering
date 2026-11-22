import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { runHost } from "../src/host.mjs";
import { acpFamily } from "../src/families/acp.mjs";
import {
  KIRO_AGENT,
  isAutopilotWrite,
  kiroConfigWrites,
  kiroFirstUserMessageId,
  kiroFork,
  kiroSteerText,
  replayKiro,
} from "./replayKiro.mjs";

// The Kiro transcripts: live Kiro 2.32.0 to 2.33.1 sessions recorded by T3 Code (MIT, Copyright
// (c) 2026 T3 Tools Inc., commit 9168ca73d02003557725430b515b44e1551544af; see
// test/fixtures/NOTICE). Every frame replays through the real ACP family with agent.id "kiro";
// replayKiro.mjs lists the recorded fields it ignores and why.

const fixture = (name) => fileURLToPath(new URL(`./fixtures/kiro/${name}.ndjson`, import.meta.url));
const WORKSPACE = "/work";
const settle = (ms = 40) => new Promise((resolve) => setTimeout(resolve, ms));
const runs = (h, sendId) => h.all.filter((frame) => frame.type === "runDone" && frame.sendId === sendId);

function boot({ spawn } = {}) {
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
  runHost({ family: acpFamily({ spawn }), input, output });
  const next = (predicate) =>
    new Promise((resolve) => {
      const check = () => {
        const index = frames.findIndex(predicate);
        if (index < 0) return false;
        resolve(frames.splice(index, 1)[0]);
        const at = waiters.indexOf(check);
        if (at >= 0) waiters.splice(at, 1);
        return true;
      };
      if (!check()) waiters.push(check);
    });
  return { next, send: (frame) => input.write(`${JSON.stringify(frame)}\n`), all };
}

async function handshake(h) {
  await h.next((frame) => frame.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((frame) => frame.id === 1);
}

function scenario(name) {
  const source = fixture(name);
  const replay = replayKiro(source);
  const h = boot({ spawn: replay.spawn });
  return { source, replay, h };
}

async function ready(name, { model, effort, mode, atTurn, operation = "create", engineSessionId, sessionId = "s", id = 2 } = {}) {
  const { source, replay, h } = scenario(name);
  await handshake(h);
  const selected = model ?? kiroConfigWrites(source, "model")[0];
  h.send({
    id,
    type: "open",
    operation,
    sessionId,
    ...(engineSessionId === undefined ? {} : { engineSessionId }),
    cwd: WORKSPACE,
    options: {
      agent: KIRO_AGENT,
      ...(selected === undefined ? {} : { model: selected }),
      ...(effort === undefined ? {} : { effort }),
      ...(mode === undefined ? {} : { mode }),
      ...(atTurn === undefined ? {} : { atTurn }),
    },
  });
  const opened = await h.next((frame) => frame.id === id);
  assert.equal(opened.ok, true, JSON.stringify(opened));
  return { source, replay, h, opened, model: selected };
}

async function turn(h, replay, index, { id = 3 + index * 10, sessionId = "s", model } = {}) {
  const text = replay.promptTexts()[index];
  assert.equal(typeof text, "string", `recorded prompt ${index}`);
  h.send({
    id,
    type: "send",
    sessionId,
    message: text,
    ...(model === undefined ? {} : { options: { model } }),
  });
  const accepted = await h.next((frame) => frame.id === id);
  assert.equal(accepted.ok, true, JSON.stringify(accepted));
  return { id, text };
}

async function close(h, sessionId = "s", id = 90) {
  h.send({ id, type: "closeSession", sessionId });
  const closed = await h.next((frame) => frame.id === id);
  assert.equal(closed.ok, true, JSON.stringify(closed));
}

function autopilotWrites(replay) {
  return [...replay.outbound, ...replay.ignoredOutbound].filter(isAutopilotWrite);
}

function assertAutopilotOff(replay) {
  const writes = autopilotWrites(replay);
  assert.ok(writes.length > 0, "Kiro offered autopilot and the host wrote its supervised value");
  assert.ok(writes.every((frame) => frame.params.value === "off"), "the host never turns Kiro autopilot on");
  assert.ok(
    !replay.outbound.some((frame) => frame.params?.configId === "autopilot" && frame.params.value !== "off"),
    "no autopilot value other than off reaches Kiro",
  );
}

// ---------------------------------------------------------------------------------------------
// Every scenario.
// ---------------------------------------------------------------------------------------------

test("kiro simple: the V3 launch, a model write, one completed run and a close", async () => {
  const { replay, h } = await ready("simple");
  assert.deepEqual(replay.spawns[0].args, ["acp", "--agent-engine=v3", "--auth-method=cli"]);
  assert.equal(replay.sent("session/set_config_option")[0].params.configId, "model");
  const { id } = await turn(h, replay, 0);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.deepEqual(done.result, { reason: "completed", threadDisposition: "reusable" });
  await settle();
  assert.equal(runs(h, id).length, 1);
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro multi_turn: one process serves both prompts", async () => {
  const { replay, h } = await ready("multi_turn");
  const first = await turn(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === first.id)).result.reason, "completed");
  const second = await turn(h, replay, 1);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id)).result.reason, "completed");
  await settle();
  assert.equal(runs(h, first.id).length, 1);
  assert.equal(runs(h, second.id).length, 1);
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro queued_turn: the second prompt is a new send after the first run ended", async () => {
  const { replay, h } = await ready("queued_turn");
  const first = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === first.id);
  const second = await turn(h, replay, 1);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id);
  await settle();
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro tool_call_read_only_on_request: both Kiro asks are permission frames answered once", async () => {
  const { replay, h } = await ready("tool_call_read_only_on_request");
  const { id } = await turn(h, replay, 0);
  const first = await h.next((frame) => frame.type === "permission" && frame.sendId === id);
  assert.equal(first.tool, "Write File");
  assert.deepEqual(first.options.map((option) => option.optionId), ["accept", "always-accept", "reject", "always-reject"]);
  h.send({ id: 30, type: "respond", requestId: first.requestId, decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((frame) => frame.id === 30)).result, null);
  const second = await h.next((frame) => frame.type === "permission" && frame.sendId === id);
  assert.equal(second.tool, "Review changes");
  h.send({ id: 31, type: "respond", requestId: second.requestId, decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((frame) => frame.id === 31)).result, null);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.deepEqual(
    replay.outboundResponses.map((entry) => [entry.method, entry.result.outcome.optionId]),
    [
      ["session/request_permission", "accept"],
      ["session/request_permission", "accept"],
    ],
  );
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro turn_interrupt: cancel sends session/cancel and ends the run cancelled once", async () => {
  const { replay, h } = await ready("turn_interrupt");
  const { id } = await turn(h, replay, 0);
  h.send({ id: 40, type: "cancel", sendId: id });
  assert.deepEqual((await h.next((frame) => frame.id === 40)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.deepEqual(done.result, { reason: "cancelled", threadDisposition: "reusable" });
  await settle();
  assert.equal(runs(h, id).length, 1);
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro turn_interrupt_mid_tool: allow once, then cancel while the tool runs", async () => {
  const { replay, h } = await ready("turn_interrupt_mid_tool");
  const { id } = await turn(h, replay, 0);
  const ask = await h.next((frame) => frame.type === "permission" && frame.sendId === id);
  h.send({ id: 41, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  await h.next((frame) => frame.id === 41);
  h.send({ id: 42, type: "cancel", sendId: id });
  assert.deepEqual((await h.next((frame) => frame.id === 42)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.deepEqual(done.result, { reason: "cancelled", threadDisposition: "reusable" });
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro subagent: Kiro's in-session subagent updates stream untouched", async () => {
  const { replay, h } = await ready("subagent");
  const { id } = await turn(h, replay, 0);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.equal(done.result.reason, "completed");
  const updates = h.all.filter((frame) => frame.type === "delta" && frame.sendId === id).map((frame) => frame.update);
  assert.ok(
    updates.some((update) => typeof update._meta?.kiro?.agentSubtaskId === "string"),
    "the subagent's own frames keep Kiro's routing metadata",
  );
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro cloud_config: the cloud-config tool call does not disturb the prompt and cancel ends the run", async () => {
  const { replay, h } = await ready("kiro_cloud_config");
  const { id } = await turn(h, replay, 0);
  // The recorded cloud-config call lands between the model write and the prompt, before this
  // host has a run for it, so the generic no-run rule leaves it out of the run's deltas (T3
  // hides it with its own filter). What matters here is that Kiro's startup fetch neither holds
  // nor breaks the turn: the recorded cancel still ends it.
  h.send({ id: 43, type: "cancel", sendId: id });
  assert.deepEqual((await h.next((frame) => frame.id === 43)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.equal(done.result.reason, "cancelled");
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro compaction: /compact uses _kiro/session/compact and the next prompt carries on", async () => {
  const { replay, h } = await ready("kiro_compaction");
  const first = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === first.id);
  h.send({ id: 50, type: "send", sessionId: "s", message: "/compact" });
  const accepted = await h.next((frame) => frame.id === 50);
  assert.equal(accepted.ok, true, JSON.stringify(accepted));
  const compacted = await h.next((frame) => frame.type === "runDone" && frame.sendId === 50);
  assert.deepEqual(compacted.result, { reason: "completed", threadDisposition: "reusable" });
  assert.deepEqual(
    replay.sent("_kiro/session/compact").map((frame) => frame.params),
    [{ sessionId: replay.sessionId() }],
  );
  const second = await turn(h, replay, 1);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id)).result.reason, "completed");
  await settle();
  assert.equal(replay.sent("session/prompt").length, 2);
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro effort: the model is written first and effortLevel after Kiro advertises it", async () => {
  const { source, replay, h } = await ready("kiro_effort", {
    model: kiroConfigWrites(fixture("kiro_effort"), "model")[0],
    effort: kiroConfigWrites(fixture("kiro_effort"), "effortLevel")[0],
  });
  const { id } = await turn(h, replay, 0);
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === id)).result.reason, "completed");
  assert.deepEqual(
    replay.outbound
      .filter((frame) => frame.method === "session/set_config_option" && frame.params.configId !== "autopilot")
      .map((frame) => [frame.params.configId, frame.params.value]),
    [
      ["model", kiroConfigWrites(source, "model")[0]],
      ["effortLevel", kiroConfigWrites(source, "effortLevel")[0]],
    ],
  );
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro fork_from_turn: a second process forks the first session at its first message", async () => {
  const { source, replay, h, model } = await ready("kiro_fork_from_turn");
  const first = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === first.id);
  const second = await turn(h, replay, 1);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id);

  const messageId = kiroFirstUserMessageId(source);
  const fork = kiroFork(source);
  assert.equal(typeof messageId, "string");
  assert.equal(typeof fork.sessionId, "string");
  h.send({
    id: 30,
    type: "open",
    operation: "fork",
    sessionId: "fork",
    engineSessionId: replay.sessionId(0),
    cwd: WORKSPACE,
    options: { agent: KIRO_AGENT, model, atTurn: messageId },
  });
  const forked = await h.next((frame) => frame.id === 30);
  assert.equal(forked.ok, true, JSON.stringify(forked));
  assert.equal(forked.result.engineSessionId, fork.sessionId);
  const forkRequest = replay.sent("session/fork")[0];
  assert.deepEqual(forkRequest.params, {
    sessionId: replay.sessionId(0),
    cwd: WORKSPACE,
    mcpServers: [],
    _meta: { kiro: { messageId } },
  });
  const third = await turn(h, replay, 2, { sessionId: "fork", id: 31 });
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === third.id)).result.reason, "completed");

  // T3 closes both sessions on the one live process at the end; the recording has the two
  // requests before either response, so send both before waiting.
  h.send({ id: 90, type: "closeSession", sessionId: "s" });
  h.send({ id: 91, type: "closeSession", sessionId: "fork" });
  assert.equal((await h.next((frame) => frame.id === 90)).ok, true);
  assert.equal((await h.next((frame) => frame.id === 91)).ok, true);
  await settle();
  assertAutopilotOff(replay);
  replay.done();
});

test("kiro message_steering: steer uses _session/steer, never cancel-and-reprompt", async () => {
  const { source, replay, h } = await ready("kiro_message_steering");
  const { id } = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "delta" && frame.sendId === id);
  const steerText = kiroSteerText(source);
  assert.equal(typeof steerText, "string");
  h.send({ id: 60, type: "steer", sendId: id, messageId: "m-1", text: steerText });
  assert.deepEqual((await h.next((frame) => frame.id === 60)).result, { outcome: "queued", messageId: "m-1" });
  assert.deepEqual(replay.sent("_session/steer").map((frame) => frame.params), [
    { sessionId: replay.sessionId(), message: steerText },
  ]);
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, id).length, 1, "a native steer never ends the run");
  assert.equal(replay.sent("session/cancel").length, 0);
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro model_switch: the live session switches model before the second prompt", async () => {
  const { source, replay, h } = await ready("kiro_model_switch");
  const first = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "runDone" && frame.sendId === first.id);
  const targets = kiroConfigWrites(source, "model");
  assert.equal(targets.length, 2);
  const second = await turn(h, replay, 1, { model: targets[1] });
  assert.equal((await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id)).result.reason, "completed");
  assert.deepEqual(
    replay.outbound
      .filter((frame) => frame.method === "session/set_config_option" && frame.params.configId === "model")
      .map((frame) => frame.params.value),
    targets,
  );
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro subagent_interrupt: cancel fails the subagent and its tool and ends one cancelled run", async () => {
  const { replay, h } = await ready("kiro_subagent_interrupt");
  const { id } = await turn(h, replay, 0);
  await h.next((frame) => frame.type === "delta" && frame.sendId === id);
  h.send({ id: 70, type: "cancel", sendId: id });
  assert.deepEqual((await h.next((frame) => frame.id === 70)).result, { cancelled: true });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === id);
  assert.equal(done.result.reason, "cancelled");
  const failed = h.all
    .filter((frame) => frame.type === "delta" && frame.sendId === id)
    .map((frame) => frame.update)
    .filter((update) => update.sessionUpdate === "tool_call_update" && update.status === "failed");
  assert.ok(failed.some((update) => update.toolCallId.startsWith("invoke_subagent_")));
  assert.ok(failed.some((update) => !update.toolCallId.startsWith("invoke_subagent_")));
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});

test("kiro unknown_model: the prompt error is a failed run, not a sign-in, and the next model recovers", async () => {
  const { source, replay, h } = await ready("kiro_unknown_model", {
    model: kiroConfigWrites(fixture("kiro_unknown_model"), "model")[0],
  });
  const failedTurn = await turn(h, replay, 0);
  const failed = await h.next((frame) => frame.type === "runDone" && frame.sendId === failedTurn.id);
  assert.equal(failed.ok, false);
  assert.equal(failed.result.reason, "failed");
  assert.equal(failed.error.cause.kind, "engine");
  assert.match(failed.error.cause.message, /not-a-kiro-model/);
  const recovered = kiroConfigWrites(source, "model")[1];
  const second = await turn(h, replay, 1, { model: recovered, id: 80 });
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === second.id);
  assert.equal(done.result.reason, "completed");
  assert.deepEqual(
    replay.outbound
      .filter((frame) => frame.method === "session/set_config_option" && frame.params.configId === "model")
      .map((frame) => frame.params.value),
    ["not-a-kiro-model", recovered],
  );
  assertAutopilotOff(replay);
  await close(h);
  replay.done();
});
