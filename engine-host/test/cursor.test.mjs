import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { PassThrough } from "node:stream";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { cursorFamily, errorKind, isBenignSdkRejection } from "../src/families/cursor.mjs";
import { runHost } from "../src/host.mjs";
import { replayCursorSdk } from "./replayCursor.mjs";

const fixture = (n) => fileURLToPath(new URL(`./fixtures/cursor/${n}.ndjson`, import.meta.url));
const transcript = (n) => readFileSync(fixture(n), "utf8").trim().split("\n").map((l) => JSON.parse(l));
const promptOf = (n) => transcript(n).find((l) => l.frame?.type === "run.start").frame.message;
const recordedOptions = (n) => transcript(n).find((l) => l.frame?.type === "agent.open").frame.options;

/** Boots the host in process around a fake SDK. `all` keeps every frame in order. */
function boot(sdk, { env = {} } = {}) {
  const input = new PassThrough();
  const output = new PassThrough();
  const all = [];
  const frames = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    const f = JSON.parse(line);
    all.push(f);
    frames.push(f);
    for (const w of [...waiters]) w();
  });
  runHost({ family: cursorFamily({ loadSdk: async () => sdk, env }), input, output });
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
  const send = (f) => input.write(`${JSON.stringify(f)}\n`);
  let id = 1;
  const call = async (type, fields = {}) => {
    const myId = ++id;
    send({ id: myId, type, ...fields });
    return { id: myId, response: await next((f) => f.type === "response" && f.id === myId) };
  };
  const ready = (async () => {
    await next((f) => f.type === "hello");
    send({ id: 1, type: "init", protocol: 1 });
    await next((f) => f.id === 1);
  })();
  return { all, next, send, call, ready, input };
}

function openOptions(name, extra = {}) {
  const o = recordedOptions(name);
  return {
    model: o.model,
    mode: o.mode,
    local: { autoReview: o.local.autoReview, settingSources: o.local.settingSources, sandboxOptions: { enabled: o.local.sandboxEnabled } },
    ...extra,
  };
}

async function openSession(h, name, sessionId = "s", extra = {}) {
  await h.ready;
  const { response } = await h.call("open", { operation: "create", sessionId, cwd: "/tmp/cursor-work", options: openOptions(name, extra) });
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.result;
}

test("simple: open, one turn, session id, ordered frames, exactly one runDone", async () => {
  const replay = replayCursorSdk(fixture("simple"));
  const h = boot(replay.sdk);
  const opened = await openSession(h, "simple");
  assert.equal(opened.engineSessionId, "agent-8f8933e5-b50d-4c9b-aaf3-5bd1fadda6f6");
  const { id, response } = await h.call("send", { sessionId: "s", message: promptOf("simple"), options: { model: { id: "composer-2.5" }, mode: "agent" } });
  assert.equal(response.ok, true);
  assert.equal(response.result.runId, "run-42618f51-5137-4daa-8d57-c514aeed4187");
  assert.equal(response.result.engineSessionId, opened.engineSessionId);
  const done = await h.next((f) => f.type === "runDone" && f.sendId === id);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "completed");
  assert.equal(done.result.threadDisposition, "reusable");
  assert.equal(done.result.usage.totalTokens > 0, true);
  const mine = h.all.filter((f) => f.sendId === id || f.id === id);
  assert.equal(mine[0].type, "response", "the response precedes every delta");
  assert.equal(mine.at(-1).type, "runDone", "runDone is last");
  assert.equal(mine.filter((f) => f.type === "runDone").length, 1);
  const updates = mine.filter((f) => f.type === "delta").map((f) => f.update.type);
  assert.ok(updates.includes("text-delta") && updates.includes("thinking-delta"), "SDK updates pass through");
  assert.equal(h.all.some((f) => f.type === "permission" || f.type === "question"), false, "Cursor has no approvals");
});

test("deltas are the SDK update, untouched, in order", async () => {
  const replay = replayCursorSdk(fixture("tool_call_read_only"));
  const h = boot(replay.sdk);
  await openSession(h, "tool_call_read_only");
  const { id } = await h.call("send", { sessionId: "s", message: promptOf("tool_call_read_only"), options: { model: { id: "composer-2.5" }, mode: "agent" } });
  await h.next((f) => f.type === "runDone" && f.sendId === id);
  const recorded = transcript("tool_call_read_only").filter((l) => l.frame?.type === "interaction.update").map((l) => l.frame.update);
  const got = h.all.filter((f) => f.type === "delta" && f.sendId === id).map((f) => f.update);
  assert.deepEqual(got, recorded);
  assert.ok(got.some((u) => u.type === "tool-call-started"));
});

test("multi_turn: two sends on one agent, two runDone", async () => {
  const replay = replayCursorSdk(fixture("multi_turn"));
  const h = boot(replay.sdk);
  await openSession(h, "multi_turn");
  const prompts = transcript("multi_turn").filter((l) => l.frame?.type === "run.start").map((l) => l.frame.message);
  assert.equal(prompts.length, 2);
  for (const p of prompts) {
    const { id } = await h.call("send", { sessionId: "s", message: p, options: { model: { id: "composer-2.5" }, mode: "agent" } });
    const done = await h.next((f) => f.type === "runDone" && f.sendId === id);
    assert.equal(done.result.reason, "completed");
  }
  assert.equal(replay.calls.opens.length, 1, "one agent for the session");
});

test("closeSession closes the agent; resume reopens it by engineSessionId", async () => {
  const replay = replayCursorSdk(fixture("provider_thread_resume"));
  const h = boot(replay.sdk);
  const opened = await openSession(h, "provider_thread_resume");
  const prompts = transcript("provider_thread_resume").filter((l) => l.frame?.type === "run.start").map((l) => l.frame.message);
  const sendOpts = { model: { id: "composer-2.5" }, mode: "agent" };
  const first = await h.call("send", { sessionId: "s", message: prompts[0], options: sendOpts });
  await h.next((f) => f.type === "runDone" && f.sendId === first.id);
  assert.equal((await h.call("closeSession", { sessionId: "s" })).response.ok, true);
  assert.deepEqual(replay.calls.closes, [opened.engineSessionId]);
  const resumed = await h.call("open", { operation: "resume", sessionId: "s", engineSessionId: opened.engineSessionId, cwd: "/tmp/cursor-work", options: openOptions("provider_thread_resume") });
  assert.equal(resumed.response.result.engineSessionId, opened.engineSessionId);
  const second = await h.call("send", { sessionId: "s", message: prompts[1], options: sendOpts });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === second.id);
  assert.equal(done.result.reason, "completed");
});

test("steer reaches run.steer; complete_delivered is accepted", async () => {
  const replay = replayCursorSdk(fixture("message_steering"));
  const h = boot(replay.sdk);
  await openSession(h, "message_steering");
  const { id } = await h.call("send", { sessionId: "s", message: promptOf("message_steering"), options: { model: { id: "composer-2.5" }, mode: "agent" } });
  await h.next((f) => f.type === "delta" && f.sendId === id);
  const steerText = transcript("message_steering").find((l) => l.frame?.type === "run.steer").frame.text;
  const { response } = await h.call("steer", { sendId: id, messageId: "m-1", text: steerText });
  assert.deepEqual(response.result, { outcome: "accepted", messageId: "m-1" });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === id);
  assert.equal(done.result.reason, "completed");
  assert.deepEqual(replay.calls.steers, [steerText]);
});

test("cancel calls run.cancel and the run ends once as cancelled", async () => {
  const replay = replayCursorSdk(fixture("turn_interrupt_mid_tool"));
  const h = boot(replay.sdk);
  await openSession(h, "turn_interrupt_mid_tool");
  const { id } = await h.call("send", { sessionId: "s", message: promptOf("turn_interrupt_mid_tool"), options: { model: { id: "composer-2.5" }, mode: "agent" } });
  await h.next((f) => f.type === "delta" && f.update.type === "tool-call-started");
  const { response } = await h.call("cancel", { sendId: id });
  assert.deepEqual(response.result, { cancelled: true });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === id);
  assert.equal(done.result.reason, "cancelled");
  await h.call("closeSession", { sessionId: "s" });
  assert.equal(replay.calls.cancels, 1);
  assert.equal(replay.calls.closes.length, 1);
  assert.equal(replay.done(), true, "the whole transcript was consumed");
  assert.equal(h.all.filter((f) => f.type === "runDone").length, 1);
});

// More T3 recordings, replayed whole: every outbound SDK call is asserted by the replay, every recorded
// update comes back as a delta in order, and each send ends with exactly one runDone.
for (const name of ["proposed_plan", "queued_turn", "skill_invocation", "subagent", "todo_list"]) {
  test(`${name}: the recorded transcript replays to completion`, async () => {
    const replay = replayCursorSdk(fixture(name));
    const h = boot(replay.sdk);
    await openSession(h, name);
    const sends = transcript(name).filter((l) => l.frame?.type === "run.start").map((l) => l.frame);
    const recorded = transcript(name).filter((l) => l.frame?.type === "interaction.update").map((l) => l.frame.update);
    let done;
    for (const f of sends) {
      const { id, response } = await h.call("send", { sessionId: "s", message: f.message, options: { model: f.options.model, mode: f.options.mode } });
      assert.equal(response.ok, true, JSON.stringify(response));
      done = await h.next((x) => x.type === "runDone" && x.sendId === id);
      assert.equal(done.result.reason, "completed");
      assert.equal(done.result.usage.totalTokens > 0, true);
    }
    assert.deepEqual(h.all.filter((x) => x.type === "delta").map((x) => x.update), recorded);
    assert.equal(h.all.filter((x) => x.type === "runDone").length, sends.length);
    assert.equal(h.all.some((x) => x.type === "permission" || x.type === "question"), false);
    await h.call("closeSession", { sessionId: "s" });
  });
}

// A hand-written fake for the cases no transcript covers (errors, pooling, confinement, the key).
function fakeSdk(behaviour = {}) {
  const calls = { creates: [], resumes: [], sends: [], closes: 0 };
  let agents = 0;
  const makeAgent = (agentId) => ({
    agentId,
    async send(message, options) {
      calls.sends.push({ agentId, message, options });
      if (behaviour.sendError) throw behaviour.sendError;
      let resolveWait;
      const wait = new Promise((r) => (resolveWait = r));
      const run = {
        id: `run-${agentId}-${calls.sends.length}`,
        wait: () => behaviour.wait?.(resolveWait, options, run) ?? wait,
        steer: async () => behaviour.steer ?? "revert_to_followup",
        cancel: async () => {
          behaviour.onCancel?.(resolveWait);
          resolveWait({ status: "cancelled" });
        },
        finish: (result) => resolveWait(result),
        emit: (update) => options.onDelta({ update }),
      };
      behaviour.onRun?.(run);
      return run;
    },
    async close() {
      calls.closes++;
    },
  });
  return {
    calls,
    sdk: {
      Agent: {
        create: async (o) => {
          calls.creates.push(o);
          if (behaviour.createError) throw behaviour.createError;
          return makeAgent(`agent-${++agents}`);
        },
        resume: async (id, o) => {
          calls.resumes.push({ id, o });
          if (behaviour.resumeError) throw behaviour.resumeError;
          return makeAgent(id);
        },
      },
    },
  };
}
const named = (name, message, extra = {}) => Object.assign(new Error(message), { name }, extra);
const basicOptions = { model: { id: "composer-2.5" }, mode: "agent", local: {} };

test("pooled: two sessions interleave on one host, each delta carries its own sendId", async () => {
  const runs = [];
  const f = fakeSdk({ onRun: (r) => runs.push(r) });
  const h = boot(f.sdk);
  await h.ready;
  const a = await h.call("open", { operation: "create", sessionId: "a", cwd: "/w/a", options: basicOptions });
  const b = await h.call("open", { operation: "create", sessionId: "b", cwd: "/w/b", options: basicOptions });
  assert.notEqual(a.response.result.engineSessionId, b.response.result.engineSessionId);
  assert.equal(f.calls.creates[0].local.cwd, "/w/a");
  assert.equal(f.calls.creates[1].local.cwd, "/w/b");
  const sa = await h.call("send", { sessionId: "a", message: "one" });
  const sb = await h.call("send", { sessionId: "b", message: "two" });
  runs[1].emit({ type: "text-delta", text: "B" });
  runs[0].emit({ type: "text-delta", text: "A" });
  runs[0].finish({ status: "finished", result: "A" });
  const doneA = await h.next((x) => x.type === "runDone" && x.sendId === sa.id);
  runs[1].finish({ status: "finished", result: "B" });
  const doneB = await h.next((x) => x.type === "runDone" && x.sendId === sb.id);
  assert.equal(doneA.result.reason, "completed");
  assert.equal(doneB.result.reason, "completed");
  const text = (sendId) => h.all.filter((x) => x.type === "delta" && x.sendId === sendId).map((x) => x.update.text);
  assert.deepEqual(text(sa.id), ["A"]);
  assert.deepEqual(text(sb.id), ["B"]);
});

test("a send while a run is open on the same session is refused", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  await h.call("send", { sessionId: "a", message: "one" });
  const second = await h.call("send", { sessionId: "a", message: "two" });
  assert.equal(second.response.ok, false);
  assert.equal(second.response.error.cause.name, "RunInProgress");
});

test("confinement.argvPrefix fails open as unsupported and creates no agent", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  const { response } = await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions, confinement: { argvPrefix: ["/usr/bin/sandbox-exec", "--"], env: { PATH: "/usr/bin" }, writeRoots: ["/w"] } });
  assert.equal(response.ok, false);
  assert.equal(response.error.cause.kind, "unsupported");
  assert.equal(f.calls.creates.length, 0);
});

test("confinement.env alone is not applied: it is logged by key, and the agent still opens", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  const { response } = await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions, confinement: { env: { PATH: "/usr/bin", SECRET_VALUE: "do-not-log-me" } } });
  assert.equal(response.ok, true);
  const log = h.all.find((x) => x.type === "log" && x.event.type === "confinementEnvNotApplied");
  assert.deepEqual(log.event.keys, ["PATH", "SECRET_VALUE"]);
  assert.equal(JSON.stringify(h.all).includes("do-not-log-me"), false);
});

test("sessions default to the SDK sandbox on; the core's own local options win", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: { model: { id: "m" }, local: {} } });
  await h.call("open", { operation: "create", sessionId: "b", cwd: "/w", options: { model: { id: "m" }, local: { sandboxOptions: { enabled: false } } } });
  assert.equal(f.calls.creates[0].local.sandboxOptions.enabled, true);
  assert.equal(f.calls.creates[1].local.sandboxOptions.enabled, false);
});

test("the API key comes only from the spawn env, reaches the SDK, and never appears in a frame", async () => {
  const key = "crsr_test_0123456789abcdef";
  const f = fakeSdk({ onRun: (r) => setImmediate(() => r.finish({ status: "error", error: named("Error", `request failed for ${key}`, { status: 500 }) })) });
  const h = boot(f.sdk, { env: { CURSOR_API_KEY: key } });
  await h.ready;
  const opened = await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  assert.equal(f.calls.creates[0].apiKey, key);
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.ok, false);
  assert.match(done.error.cause.message, /\[redacted\]/);
  const bad = await h.call("open", { operation: "create", sessionId: "b", cwd: "/w", options: { ...basicOptions, apiKey: "from-the-wire" } });
  assert.equal(bad.response.ok, false);
  assert.equal(bad.response.error.cause.kind, "invalid_request");
  assert.equal(JSON.stringify(h.all).includes(key), false);
  assert.equal(JSON.stringify(h.all).includes("from-the-wire"), false);
  assert.equal(opened.response.ok, true);
});

test("an SDK error on open carries its kind and a redacted message", async () => {
  const key = "crsr_test_abcdefghijklmnop";
  const cases = [
    [named("AuthenticationError", `bad key ${key}`), "auth"],
    [named("Error", "rate limit reached", { status: 429 }), "rate_limit"],
    [named("ConfigurationError", "Cannot use this model: x"), "invalid_request"],
    [named("Error", "connect failed", { code: "ECONNRESET" }), "network"],
    [named("CursorSdkError", "something odd"), "engine"],
  ];
  for (const [error, kind] of cases) {
    const h = boot(fakeSdk({ createError: error }).sdk, { env: { CURSOR_API_KEY: key } });
    await h.ready;
    const { response } = await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
    assert.equal(response.ok, false);
    assert.equal(response.error.method, "open");
    assert.equal(response.error.cause.kind, kind, error.name);
    assert.equal(JSON.stringify(response).includes(key), false);
  }
});

test("a resume the SDK cannot find is session_lost", async () => {
  const h = boot(fakeSdk({ resumeError: named("CursorSdkError", "Agent agent-9 not found") }).sdk);
  await h.ready;
  const { response } = await h.call("open", { operation: "resume", sessionId: "a", engineSessionId: "agent-9", cwd: "/w", options: basicOptions });
  assert.equal(response.error.cause.kind, "session_lost");
});

test("fork is unsupported", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  const { response } = await h.call("open", { operation: "fork", sessionId: "a", engineSessionId: "x", cwd: "/w", options: basicOptions });
  assert.equal(response.error.cause.kind, "unsupported");
  assert.equal(f.calls.creates.length + f.calls.resumes.length, 0);
});

test("a failed run result ends once with kind and a retry hint for a rate limit", async () => {
  const f = fakeSdk({ onRun: (r) => setImmediate(() => r.finish({ status: "error", error: named("Error", "Usage limit reached", { status: 429 }) })) });
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.ok, false);
  assert.equal(done.error.cause.kind, "rate_limit");
  assert.equal(done.result.reason, "failed");
  assert.equal(done.result.retry.why, "rate_limit");
});

test("run.wait rejecting ends the run as failed, once", async () => {
  const f = fakeSdk({ wait: async () => { throw named("Error", "stream broke", { code: "ECONNRESET" }); } });
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.error.cause.kind, "network");
  assert.equal(h.all.filter((x) => x.type === "runDone").length, 1);
});

test("the no-progress resume error after a turn ended counts as completed", async () => {
  const f = fakeSdk({
    onRun: (r) => setImmediate(async () => {
      await r.emit({ type: "turn-ended" });
      r.finish({ status: "error", error: named("Error", "Agent turn stopped after repeated resume attempts made no progress") });
    }),
  });
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "completed");
});

test("a send the SDK refuses fails the response with a kind and sends no runDone", async () => {
  const h = boot(fakeSdk({ sendError: named("AuthenticationError", "login expired") }).sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  assert.equal(sent.response.ok, false);
  assert.equal(sent.response.error.cause.kind, "auth");
  assert.equal(h.all.some((x) => x.type === "runDone"), false);
  const again = await h.call("send", { sessionId: "a", message: "hi" });
  assert.equal(again.response.error.cause.kind, "auth", "the session stays usable after a failed send");
});

test("closeSession with an open run ends it cancelled, once, and closes the agent", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  await h.call("closeSession", { sessionId: "a" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.result.reason, "cancelled");
  assert.equal(f.calls.closes, 1);
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(h.all.filter((x) => x.type === "runDone").length, 1);
});

test("a steer the SDK declines is rejected so the core sends a follow-up", async () => {
  const f = fakeSdk();
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const { response } = await h.call("steer", { sendId: sent.id, messageId: "m", text: "more" });
  assert.deepEqual(response.result, { outcome: "rejected", messageId: "m" });
});

test("respond has nothing to answer: it is logged and ignored", async () => {
  const h = boot(fakeSdk().sdk);
  await h.ready;
  const { response } = await h.call("respond", { requestId: "p-1", decision: { behavior: "allow" } });
  assert.equal(response.ok, true);
  assert.equal(h.all.some((x) => x.type === "log" && x.event.type === "lateRespond"), true);
});

test("capabilities list steer only; usage and the rest are not claimed", async () => {
  const h = boot(fakeSdk().sdk);
  await h.ready;
  const hello = h.all.find((f) => f.type === "hello");
  assert.equal(hello.family, "cursor");
  assert.equal(hello.sdk.name, "@cursor/sdk");
  assert.deepEqual(hello.capabilities, ["steer"]);
  const { response } = await h.call("usage", { sessionId: "a" });
  assert.equal(response.error.cause.name, "UnsupportedRequest");
});

test("error kinds and benign SDK rejections", () => {
  assert.equal(errorKind(named("AbortError", "x")), "cancelled");
  assert.equal(errorKind(Object.assign(new Error("outer"), { cause: named("AuthenticationError", "inner") })), "auth");
  assert.equal(isBenignSdkRejection(named("WriteIterableClosedError", "closed")), true);
  assert.equal(isBenignSdkRejection(new Error("real bug")), false);
  assert.equal(isBenignSdkRejection("not an error"), false);
});

// --- main.mjs: the family exists only behind the env flag, and the SDK is never a dependency ---

const main = fileURLToPath(new URL("../src/main.mjs", import.meta.url));
function startMain(env) {
  const base = { ...process.env };
  delete base.ORBYN_ENGINE_HOST_CURSOR;
  return spawn(process.execPath, [main, "cursor"], { env: { ...base, ...env }, stdio: ["pipe", "pipe", "pipe"] });
}
const lineOf = (child) => new Promise((resolve) => createInterface({ input: child.stdout }).once("line", resolve));
const exited = (child) => new Promise((resolve) => child.once("exit", (code) => resolve(code)));

test("without ORBYN_ENGINE_HOST_CURSOR=1 the family is unknown and the host exits 2", async () => {
  const child = startMain({});
  let err = "";
  child.stderr.on("data", (d) => (err += d));
  assert.equal(await exited(child), 2);
  assert.match(err, /unknown family: cursor/);
});

test("with the flag the host says hello, and a missing SDK fails open with a plain message", async () => {
  const child = startMain({ ORBYN_ENGINE_HOST_CURSOR: "1" });
  const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const hello = JSON.parse((await lines.next()).value);
  assert.equal(hello.family, "cursor");
  child.stdin.write(`${JSON.stringify({ id: 1, type: "init", protocol: 1 })}\n`);
  assert.equal(JSON.parse((await lines.next()).value).ok, true);
  child.stdin.write(`${JSON.stringify({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: process.cwd(), options: {} })}\n`);
  const res = JSON.parse((await lines.next()).value);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.message, "The Cursor SDK is not available in this build.");
  assert.equal(res.error.cause.kind, "unsupported");
  child.stdin.end();
  assert.equal(await exited(child), 0);
});

test("package.json does not depend on @cursor/sdk, even optionally", () => {
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  for (const key of ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"]) {
    assert.equal(Object.keys(pkg[key] ?? {}).includes("@cursor/sdk"), false, key);
  }
  const build = readFileSync(new URL("../scripts/build-sea.mjs", import.meta.url), "utf8");
  assert.match(build, /external:\s*\[[^\]]*"@cursor\/sdk"/);
});

test("without result usage, the summed turn-ended usage is reported (cancel and the no-progress completion)", async () => {
  const usage = { inputTokens: 10, outputTokens: 2, cacheReadTokens: 5, cacheWriteTokens: 1 };
  const f = fakeSdk({
    onRun: (r) => setImmediate(async () => {
      await r.emit({ type: "turn-ended", usage });
      await r.emit({ type: "turn-ended", usage });
      r.finish({ status: "error", error: named("Error", "Agent turn stopped after repeated resume attempts made no progress") });
    }),
  });
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.equal(done.result.reason, "completed");
  assert.deepEqual(done.result.usage, { inputTokens: 20, outputTokens: 4, cacheReadTokens: 10, cacheWriteTokens: 2, totalTokens: 36 });
});

test("the result's own usage wins over the summed turn-ended usage", async () => {
  const own = { inputTokens: 1, outputTokens: 1, totalTokens: 2 };
  const f = fakeSdk({
    onRun: (r) => setImmediate(async () => {
      await r.emit({ type: "turn-ended", usage: { inputTokens: 9, outputTokens: 9 } });
      r.finish({ status: "finished", usage: own });
    }),
  });
  const h = boot(f.sdk);
  await h.ready;
  await h.call("open", { operation: "create", sessionId: "a", cwd: "/w", options: basicOptions });
  const sent = await h.call("send", { sessionId: "a", message: "hi" });
  const done = await h.next((x) => x.type === "runDone" && x.sendId === sent.id);
  assert.deepEqual(done.result.usage, own);
});
