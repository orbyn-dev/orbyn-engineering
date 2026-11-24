import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { opencodeFamily, SDK_VERSION } from "../src/families/opencode.mjs";
import { runHost } from "../src/host.mjs";
import { AsyncQueue } from "../src/asyncQueue.mjs";
import { replayOpenCode } from "./replayOpenCode.mjs";

const fixture = (n) => fileURLToPath(new URL(`./fixtures/opencode/${n}.ndjson`, import.meta.url));

function boot({ client, launch, grace = 25 }) {
  const launches = [];
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const all = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    const f = JSON.parse(line);
    all.push(f);
    frames.push(f);
    for (const w of [...waiters]) w();
  });
  const stops = { count: 0 };
  runHost({
    family: opencodeFamily({
      wakeGraceMs: grace,
      loadSdk: async () => ({ OpenCode: { make: (opts) => { launches.at(-1).make = opts; return client; } } }),
      launch: launch ?? (async (spec) => {
        launches.push({ spec });
        return { url: "http://127.0.0.1:1", password: "pw", exited: new Promise(() => {}), stop: () => stops.count++ };
      }),
    }),
    input,
    output,
  });
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
  return { next, send, all, launches, stops, input };
}

async function handshake(h) {
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((f) => f.id === 1);
}

async function open(h, extra = {}, id = 2, sessionId = "s") {
  h.send({ id, type: "open", operation: "create", sessionId, cwd: "/work", options: { model: { id: "big-pickle", providerID: "opencode" } }, ...extra });
  const res = await h.next((f) => f.id === id);
  assert.equal(res.ok, true, JSON.stringify(res));
  return res;
}

const replayHost = (name) => {
  const replay = replayOpenCode(fixture(name));
  const h = boot({ client: replay.client });
  return { replay, h };
};
const runs = (h, sendId) => h.all.filter((f) => f.type === "runDone" && f.sendId === sendId);
const settle = () => new Promise((r) => setTimeout(r, 30));

test("hello names the family and the pinned SDK version", async () => {
  const { h } = replayHost("permission");
  const hello = await h.next((f) => f.type === "hello");
  assert.equal(hello.family, "opencode");
  assert.equal(hello.sdk.name, "@opencode/client");
  assert.deepEqual(hello.capabilities, ["steer", "fork", "models", "usage", "readHistory", "stopTask", "background"]);
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.equal(pkg.dependencies["@opencode/client"], SDK_VERSION);
  assert.equal(hello.sdk.version, SDK_VERSION);
});

test("permission: every ask is a frame, allow is once-only, deny is reject with the message, one runDone", async () => {
  const { replay, h } = replayHost("permission");
  await handshake(h);
  const opened = await open(h);
  assert.equal(opened.result.engineSessionId, "ses_f1483bd0affez3sG5IbKgOai7e");
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const accepted = await h.next((f) => f.id === 3);
  assert.equal(accepted.ok, true);
  assert.ok(accepted.result.turnId, "the user message id is the turn id");

  const first = await h.next((f) => f.type === "permission");
  assert.equal(first.sendId, 3);
  assert.equal(first.tool, "shell");
  assert.deepEqual(first.input.resources, ["echo FIRST"]);
  assert.equal("sessionID" in first.input, false, "bookkeeping ids live on the frame");
  h.send({ id: 4, type: "respond", requestId: first.requestId, decision: { behavior: "allow" } });

  for (const id of [5, 6, 7]) {
    const ask = await h.next((f) => f.type === "permission");
    h.send({ id, type: "respond", requestId: ask.requestId, decision: { behavior: "deny", message: "No more shell here." } });
  }
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.sendId, 3);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "completed");
  assert.equal(done.result.threadDisposition, "reusable");
  assert.ok(done.result.usage.input > 0);
  await settle();
  assert.equal(runs(h, 3).length, 1, "runDone exactly once");

  assert.deepEqual(replay.calls.replies.map((r) => r.decision), ["once", "reject", "reject", "reject"]);
  assert.ok(replay.calls.replies.every((r) => r.decision !== "always"));
  assert.equal(replay.calls.replies[1].message, "No more shell here.");
  assert.equal("message" in replay.calls.replies[0], false);
  assert.deepEqual(replay.calls.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("a late or unknown respond is ignored and logged", async () => {
  const { h } = replayHost("permission");
  await handshake(h);
  h.send({ id: 3, type: "respond", requestId: "p-99", decision: { behavior: "allow" } });
  assert.equal((await h.next((f) => f.id === 3)).ok, true);
  const log = await h.next((f) => f.type === "log" && f.event.type === "lateRespond");
  assert.equal(log.event.requestId, "p-99");
});

test("interrupt: cancel interrupts the session and the run ends once as cancelled", async () => {
  const { replay, h } = replayHost("interrupt");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  await h.next((f) => f.type === "delta" && f.update.type === "session.tool.progress");
  h.send({ id: 4, type: "cancel", sendId: 3 });
  const res = await h.next((f) => f.id === 4);
  assert.deepEqual(res.result, { cancelled: true });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "cancelled");
  assert.equal(done.result.threadDisposition, "reusable");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.calls.interrupts, [{ sessionID: "ses_f148c8240ffegmJ5ZhqhyYOcSH" }]);
  assert.deepEqual(replay.calls.failures, []);
});

test("cancel of an unknown run is not cancelled", async () => {
  const { h } = replayHost("interrupt");
  await handshake(h);
  h.send({ id: 3, type: "cancel", sendId: 42 });
  assert.deepEqual((await h.next((f) => f.id === 3)).result, { cancelled: false });
});

test("inbox: a steer reaches the running turn, then a second turn runs on its own", async () => {
  const { replay, h } = replayHost("inbox");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  await h.next((f) => f.type === "delta" && f.update.type === "session.tool.progress");
  h.send({ id: 4, type: "steer", sendId: 3, messageId: "m-1", text: replay.promptTexts()[1] });
  const steer = await h.next((f) => f.id === 4);
  assert.deepEqual(steer.result, { outcome: "queued", messageId: "m-1" });
  const done1 = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done1.result.reason, "completed");
  assert.equal(replay.calls.prompts[1].delivery, "steer");
  assert.equal(replay.calls.prompts[1].sessionID, "ses_f14854c71ffeuPLxKSK70AHWvd");

  h.send({ id: 5, type: "send", sessionId: "s", message: replay.promptTexts()[2] });
  const done2 = await h.next((f) => f.type === "runDone" && f.sendId === 5);
  assert.equal(done2.result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.equal(runs(h, 5).length, 1);
  assert.deepEqual(replay.calls.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("a steer for a run that is not open is rejected", async () => {
  const { h } = replayHost("inbox");
  await handshake(h);
  h.send({ id: 3, type: "steer", sendId: 9, messageId: "m-2", text: "hello" });
  assert.deepEqual((await h.next((f) => f.id === 3)).result, { outcome: "rejected", messageId: "m-2" });
});

test("fork: two turns, fork before the second, resume the fork and run a turn there", async () => {
  const { replay, h } = replayHost("fork");
  await handshake(h);
  await open(h);
  const texts = replay.promptTexts();
  h.send({ id: 3, type: "send", sessionId: "s", message: texts[0] });
  await h.next((f) => f.type === "runDone" && f.sendId === 3);
  h.send({ id: 4, type: "send", sessionId: "s", message: texts[1] });
  const second = await h.next((f) => f.id === 4);
  await h.next((f) => f.type === "runDone" && f.sendId === 4);

  h.send({ id: 5, type: "fork", sessionId: "s", atTurn: second.result.turnId });
  const forked = await h.next((f) => f.id === 5);
  assert.equal(forked.ok, true, JSON.stringify(forked));
  assert.equal(forked.result.engineSessionId, "ses_f1484db83ffeLGtrRCFimo1H0e");
  assert.deepEqual(replay.calls.forks, [{ sessionID: "ses_f1484ea8affe5aTo8CcgbGIszj", before: replay.promptIds()[1] }]);
  assert.deepEqual(replay.calls.updates[0].permissions, [{ action: "*", resource: "*", effect: "ask" }], "the fork asks for everything again");

  await open(h, { operation: "resume", engineSessionId: forked.result.engineSessionId }, 6, "f");
  h.send({ id: 7, type: "send", sessionId: "f", message: texts[2] });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 7);
  assert.equal(done.result.reason, "completed");
  assert.equal(replay.calls.prompts.at(-1).sessionID, "ses_f1484db83ffeLGtrRCFimo1H0e");
  assert.deepEqual(replay.calls.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("open with fork forks the engine session before a turn and asks for everything", async () => {
  const { h, replay } = replayHost("fork");
  await handshake(h);
  await open(h);
  const texts = replay.promptTexts();
  for (const [id, text] of [[3, texts[0]], [4, texts[1]]]) {
    h.send({ id, type: "send", sessionId: "s", message: text });
    await h.next((f) => f.type === "runDone" && f.sendId === id);
  }
  const res = await open(h, { operation: "fork", engineSessionId: "ses_f1484ea8affe5aTo8CcgbGIszj", options: { before: replay.promptIds()[1] } }, 5, "f");
  assert.equal(res.result.engineSessionId, "ses_f1484db83ffeLGtrRCFimo1H0e");
  assert.deepEqual(replay.calls.forks, [{ sessionID: "ses_f1484ea8affe5aTo8CcgbGIszj", before: replay.promptIds()[1] }]);
  assert.deepEqual(replay.calls.updates[0].permissions, [{ action: "*", resource: "*", effect: "ask" }]);
  h.send({ id: 6, type: "send", sessionId: "f", message: texts[2] });
  assert.equal((await h.next((f) => f.type === "runDone" && f.sendId === 6)).result.reason, "completed");
  assert.deepEqual(replay.calls.failures, []);
});

test("resume of a session the server does not know fails with session_lost", async () => {
  const client = {
    server: { info: async () => ({ version: "2.0.18" }) },
    session: { get: async () => { throw Object.assign(new Error("not found"), { name: "ClientError", reason: "UnexpectedStatus", status: 404 }); } },
    event: { subscribe: () => { const q = new AsyncQueue(); q.push({ type: "server.connected", data: {} }); return q; } },
  };
  const h = boot({ client });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "resume", sessionId: "s", engineSessionId: "ses_gone", cwd: "/work", options: {} });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "session_lost");
});

test("confinement: the server starts under the argv prefix with only the allowlisted environment", async () => {
  process.env.ORBYN_TEST_SECRET = "leak";
  const { replay, h } = replayHost("interrupt");
  await handshake(h);
  await open(h, {
    confinement: { argvPrefix: ["/usr/bin/sandbox-exec", "-f", "/p.sb", "--"], env: { PATH: "/usr/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1", OPENCODE_SERVER_PASSWORD: "inherited" }, cwd: "/work/wt", writeRoots: ["/work/wt"] },
    options: { opencodePath: "/opt/opencode" },
  });
  delete process.env.ORBYN_TEST_SECRET;
  const { spec } = h.launches[0];
  assert.equal(spec.command, "/usr/bin/sandbox-exec");
  assert.deepEqual(spec.args, ["-f", "/p.sb", "--", "/opt/opencode", "serve", "--hostname=127.0.0.1", "--port=0"]);
  assert.equal(spec.cwd, "/work/wt");
  assert.equal(spec.env.PATH, "/usr/bin");
  assert.equal(spec.env.ORBYN_WORKER_ID, "w-1");
  assert.equal(typeof spec.env.OPENCODE_PASSWORD, "string");
  assert.ok(spec.env.OPENCODE_PASSWORD.length >= 32);
  assert.equal("OPENCODE_SERVER_PASSWORD" in spec.env, false);
  assert.equal("ORBYN_TEST_SECRET" in spec.env, false);
  const log = await h.next((f) => f.type === "log" && f.event.type === "envAdded");
  assert.deepEqual(log.event.keys, ["OPENCODE_PASSWORD"]);
  assert.equal(JSON.stringify(h.all).includes(spec.env.OPENCODE_PASSWORD), false, "the server password never reaches a frame");
  assert.equal(h.launches[0].make.headers.Authorization, `Basic ${Buffer.from(`opencode:${spec.env.OPENCODE_PASSWORD}`).toString("base64")}`);
  assert.ok(replay);
});

test("without confinement the server starts plainly with the host environment", async () => {
  const { h } = replayHost("interrupt");
  await handshake(h);
  await open(h);
  const { spec } = h.launches[0];
  assert.equal(spec.command, "opencode");
  assert.deepEqual(spec.args, ["serve", "--hostname=127.0.0.1", "--port=0"]);
});

test("a malformed argv prefix is an invalid request and starts nothing", async () => {
  const { h } = replayHost("interrupt");
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/work", options: {}, confinement: { argvPrefix: "sandbox-exec" } });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "invalid_request");
  assert.equal(h.launches.length, 0);
});

test("attaching to a running server cannot carry a confinement prefix: unsupported", async () => {
  const { h } = replayHost("interrupt");
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/work", options: { serverUrl: "http://127.0.0.1:4096" }, confinement: { argvPrefix: ["x"] } });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "unsupported");
  assert.equal(h.launches.length, 0);
});

test("a missing opencode binary fails open with a plain message", async () => {
  const h = boot({
    client: {},
    launch: async () => { throw Object.assign(new Error("spawn opencode ENOENT"), { code: "ENOENT" }); },
  });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/work", options: {} });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "engine");
  assert.match(res.error.cause.message, /OpenCode/);
});

function scripted(script) {
  const events = new AsyncQueue();
  events.push({ type: "server.connected", data: {} });
  const client = {
    server: { info: async () => ({ version: "2.0.18" }) },
    session: {
      create: async () => ({ id: "ses_x" }),
      prompt: async () => {
        queueMicrotask(() => script.forEach((e) => events.push(e)));
        return { id: "msg_1" };
      },
      interrupt: async () => ({ interrupted: true }),
    },
    permission: { reply: async () => {} },
    event: { subscribe: () => events },
  };
  return { client, events };
}

for (const [name, error, kind, retry] of [
  ["auth", { type: "provider_error", message: "bad key", status: 401 }, "auth", undefined],
  ["rate limit", { type: "provider_error", message: "slow down", status: 429 }, "rate_limit", { why: "rate_limit" }],
  ["engine", { type: "unknown", message: "boom" }, "engine", undefined],
]) {
  test(`execution.failed (${name}) ends the run once with kind ${kind}`, async () => {
    const { client } = scripted([{ type: "session.execution.failed", data: { sessionID: "ses_x", error } }]);
    const h = boot({ client });
    await handshake(h);
    await open(h);
    h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
    const done = await h.next((f) => f.type === "runDone");
    assert.equal(done.ok, false);
    assert.equal(done.result.reason, "failed");
    assert.equal(done.error.cause.kind, kind);
    assert.deepEqual(done.result.retry, retry);
    await settle();
    assert.equal(runs(h, 3).length, 1);
  });
}

test("the event stream ending mid-run fails the run as broken, never silently", async () => {
  const { client, events } = scripted([]);
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  events.close();
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.ok, false);
  assert.equal(done.result.threadDisposition, "broken");
  assert.equal(done.error.cause.kind, "engine");
});

test("closeSession and close stop the servers they started", async () => {
  const { client } = scripted([]);
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "closeSession", sessionId: "s" });
  assert.equal((await h.next((f) => f.id === 3)).ok, true);
  h.send({ id: 4, type: "close" });
  await h.next((f) => f.id === 4);
  await settle();
  assert.equal(h.stops.count, 1);
});

// ---- models, usage, history, stopTask, background, background frames, question frames ----

function fake(over = {}) {
  const events = new AsyncQueue();
  events.push({ type: "server.connected", data: {} });
  const calls = { forms: [], cancels: [], replies: [], interrupts: [], removes: [], backgrounds: [], messageLists: [], modelLists: [], gets: [] };
  const client = {
    server: { info: async () => ({ version: "2.0.18" }) },
    session: {
      create: async () => ({ id: "ses_x" }),
      get: async (i) => (calls.gets.push(i), { id: i.sessionID }),
      prompt: async () => ({ id: "msg_1" }),
      interrupt: async (i) => (calls.interrupts.push(i), { interrupted: true }),
      background: async (i) => void calls.backgrounds.push(i),
      form: { reply: async (i) => void calls.forms.push(i), cancel: async (i) => void calls.cancels.push(i) },
    },
    permission: { reply: async (i) => void calls.replies.push(i) },
    shell: { remove: async (i) => void calls.removes.push(i) },
    model: { list: async (i) => (calls.modelLists.push(i), { location: { directory: "/work" }, data: [] }), default: async () => ({ location: { directory: "/work" }, data: null }) },
    message: { list: async (i) => (calls.messageLists.push(i), { data: [], cursor: {} }) },
    event: { subscribe: () => events },
  };
  for (const key of ["session", "model", "message"]) client[key] = { ...client[key], ...(over[key] ?? {}) };
  return { client, events, calls };
}

const ev = (type, data) => ({ type, data });
const answeredRun = async (h, sendId = 3) => {
  h.send({ id: sendId, type: "send", sessionId: "s", message: "go" });
  await h.next((f) => f.id === sendId);
};

test("models: lists the server's models with efforts, no settings, headers or keys, and stops a server it started alone", async () => {
  const replay = replayOpenCode(fixture("question"));
  const h = boot({ client: replay.client });
  await handshake(h);
  h.send({ id: 2, type: "models", cwd: "/work" });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, true, JSON.stringify(res));
  assert.deepEqual(res.result.map((m) => [m.providerID, m.id]), [["opencode", "space-bunny-free"], ["opencode", "big-pickle"]]);
  assert.deepEqual(res.result[0].efforts, ["low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(res.result[1].efforts, []);
  assert.equal(res.result[0].name, "Space Bunny Free");
  assert.deepEqual(res.result[0].capabilities, { tools: true, input: ["text", "image", "video"], output: ["text"] });
  assert.deepEqual(res.result[0].limit, { context: 1048576, input: 524288, output: 524288 });
  for (const model of res.result) for (const key of ["settings", "headers", "body", "package"]) assert.equal(key in model, false, `${key} is not forwarded`);
  assert.equal(JSON.stringify(res).includes("apiKey"), false);
  assert.deepEqual(replay.calls.modelLists, [{ location: { directory: "/work" } }]);
  await settle();
  assert.equal(h.stops.count, 1, "a server started only for the list is stopped");
});

test("models: marks the server's default and reuses a running server without stopping it", async () => {
  const { client, calls } = fake({
    model: {
      list: async () => ({ data: [{ id: "a", providerID: "p", name: "A", capabilities: {}, variants: [{ id: "high", settings: { reasoningEffort: "high" } }], limit: { context: 10, output: 5 }, enabled: true, status: "active" }, { id: "b", providerID: "p", name: "B", capabilities: {}, variants: [], limit: { context: 10, output: 5 }, enabled: false, status: "deprecated" }] }),
      default: async () => ({ data: { id: "b", providerID: "p" } }),
    },
  });
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "models" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(h.launches.length, 1, "the open session's server answered");
  assert.deepEqual(res.result.map((m) => [m.id, m.default === true, m.enabled, m.status]), [["a", false, true, "active"], ["b", true, false, "deprecated"]]);
  assert.deepEqual(res.result[0].efforts, ["high"]);
  await settle();
  assert.equal(h.stops.count, 0);
  assert.ok(calls);
});

test("usage: cumulative tokens and cost from the session, context from the last step against the model's window", async () => {
  const { client, events } = fake({
    session: { get: async () => ({ id: "ses_x", cost: 0.5, tokens: { input: 100, output: 20, reasoning: 3, cache: { read: 10, write: 4 } }, model: { id: "m", providerID: "p" } }) },
    model: { list: async () => ({ data: [{ id: "m", providerID: "p", name: "M", capabilities: {}, variants: [], limit: { context: 200000, input: 150000, output: 8000 } }] }) },
  });
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  events.push(ev("session.step.ended", { sessionID: "ses_x", finish: "stop", cost: 0, tokens: { input: 50, output: 5, reasoning: 1, cache: { read: 30, write: 2 } } }));
  await h.next((f) => f.type === "delta" && f.update.type === "session.step.ended");
  h.send({ id: 4, type: "usage", sessionId: "s" });
  const res = await h.next((f) => f.id === 4);
  assert.equal(res.ok, true, JSON.stringify(res));
  assert.deepEqual(res.result, { input: 100, output: 20, reasoning: 3, cache: { read: 10, write: 4 }, costUsd: 0.5, model: { id: "m", providerID: "p" }, context: { usedTokens: 87, maxTokens: 150000 } });
});

test("usage: without a step seen (a resumed session) the context comes from the last assistant message; unknown window is null", async () => {
  const { client, calls } = fake({
    session: { get: async () => ({ id: "ses_old", cost: 0, tokens: { input: 1, output: 2, reasoning: 0, cache: { read: 0, write: 0 } }, model: { id: "m", providerID: "p" } }) },
    message: { list: async (i) => (calls.messageLists.push(i), { data: [{ id: "msg_a", type: "assistant", tokens: { input: 7, output: 3, reasoning: 0, cache: { read: 20, write: 0 } } }], cursor: {} }) },
  });
  const h = boot({ client });
  await handshake(h);
  await open(h, { operation: "resume", engineSessionId: "ses_old" });
  h.send({ id: 3, type: "usage", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.deepEqual(res.result.context, { usedTokens: 30, maxTokens: null });
  assert.deepEqual(calls.messageLists, [{ sessionID: "ses_old", order: "desc", limit: 1, type: "assistant" }]);
});

test("usage of an unknown session is an invalid request", async () => {
  const { client } = fake();
  const h = boot({ client });
  await handshake(h);
  h.send({ id: 2, type: "usage", sessionId: "nope" });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "invalid_request");
});

test("readHistory: pages through the full message log in order and returns the engine's messages untouched", async () => {
  const replay = replayOpenCode(fixture("resume_after_restart"));
  const recorded = replay.recorded("message.list").data;
  const second = { id: "msg_later", type: "user", time: { created: 1 }, text: "more" };
  const pages = [{ data: recorded, cursor: { next: "c1" } }, { data: [second], cursor: {} }];
  const seen = [];
  const { client } = fake({ message: { list: async (i) => (seen.push(i), pages.shift()) } });
  const h = boot({ client });
  await handshake(h);
  await open(h, { operation: "resume", engineSessionId: "ses_old" });
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(res.ok, true, JSON.stringify(res));
  assert.deepEqual(res.result, [...recorded, second]);
  assert.deepEqual(seen, [{ sessionID: "ses_old", order: "asc", limit: 100 }, { sessionID: "ses_old", limit: 100, cursor: "c1" }], "cursor pages omit order");
});

test("readHistory: a history too large for one frame fails plainly instead of being cut", async () => {
  const big = { id: "msg_big", type: "assistant", content: [{ type: "text", text: "x".repeat(1024 * 1024) }] };
  const { client } = fake({ message: { list: async () => ({ data: Array.from({ length: 7 }, () => big), cursor: {} }) } });
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "host");
  assert.match(res.error.cause.message, /too large/i);
});

test("stopTask: a shell is removed in the session's directory, a subagent is interrupted; unknown tasks are not stopped", async () => {
  const { client, events, calls } = fake({ session: { interrupt: async (i) => (calls.interrupts.push(i), { interrupted: i.sessionID === "ses_c" }) } });
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  events.push(ev("session.tool.input.started", { sessionID: "ses_x", id: "call_a", name: "subagent" }));
  events.push(ev("session.tool.called", { sessionID: "ses_x", id: "call_a", input: { agent: "general", description: "Child", background: true } }));
  events.push(ev("session.created", { sessionID: "ses_c", parentID: "ses_x", title: "Child", agent: "general" }));
  events.push(ev("session.tool.progress", { sessionID: "ses_x", id: "call_a", metadata: { sessionID: "ses_c", status: "running" } }));
  events.push(ev("shell.created", { info: { id: "sh_1", status: "running", command: "sleep 60", cwd: "/work", metadata: { sessionID: "ses_x" } } }));
  events.push(ev("session.tool.progress", { sessionID: "ses_x", id: "call_b", metadata: { shellID: "sh_1" } }));
  await h.next((f) => f.type === "background" && f.event.id === "sh_1");

  h.send({ id: 4, type: "stopTask", sessionId: "s", taskId: "sh_1" });
  assert.deepEqual((await h.next((f) => f.id === 4)).result, { stopped: true });
  assert.deepEqual(calls.removes, [{ id: "sh_1", location: { directory: "/work" } }]);
  h.send({ id: 5, type: "stopTask", sessionId: "s", taskId: "ses_c" });
  assert.deepEqual((await h.next((f) => f.id === 5)).result, { stopped: true });
  assert.deepEqual(calls.interrupts, [{ sessionID: "ses_c" }]);
  h.send({ id: 6, type: "stopTask", sessionId: "s", taskId: "sh_unknown" });
  assert.deepEqual((await h.next((f) => f.id === 6)).result, { stopped: false });
  assert.equal(calls.removes.length, 1);
});

test("background: asks the server to background the run's session; no open run is not backgrounded", async () => {
  const { client, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "background", sendId: 3 });
  assert.deepEqual((await h.next((f) => f.id === 3)).result, { backgrounded: false });
  assert.deepEqual(calls.backgrounds, []);
  h.send({ id: 4, type: "send", sessionId: "s", message: "go" });
  await h.next((f) => f.id === 4);
  h.send({ id: 5, type: "background", sendId: 4 });
  assert.deepEqual((await h.next((f) => f.id === 5)).result, { backgrounded: true });
  assert.deepEqual(calls.backgrounds, [{ sessionID: "ses_x" }]);
});

const backgroundFrames = (h) => h.all.filter((f) => f.type === "background");
const indexOfFrame = (h, pred) => h.all.findIndex(pred);

test("background transcript: shell and subagent frames with tool call ids; the run is held past the first result until the wake turn ends it", async () => {
  const { replay, h } = replayHost("background");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.sendId, 3);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1, "runDone exactly once");

  const frames = backgroundFrames(h);
  assert.equal(frames.every((f) => f.sessionId === "s"), true);
  const child = "ses_f1485c529ffea4URrYruwEg0Ja";
  const shell = "sh_0eb7a4324001OUYfV2HeA220HO";
  assert.deepEqual(frames.map((f) => [f.event.type, f.event.kind, f.event.id]), [["started", "agent", child], ["started", "shell", shell], ["ended", "shell", shell], ["ended", "agent", child]]);
  assert.deepEqual(frames[0].event, { type: "started", id: child, kind: "agent", toolCallId: "call-cccf2157-e8a3-456c-a74f-fccd26fb83eb", description: "Sleep and reply", background: true });
  assert.deepEqual(frames[1].event, { type: "started", id: shell, kind: "shell", toolCallId: "call_function_s6bjpbvhrlpl_1", command: "sleep 20" });
  assert.deepEqual(frames[2].event, { type: "ended", id: shell, kind: "shell", status: "exited", exit: 0 });
  assert.deepEqual(frames[3].event, { type: "ended", id: child, kind: "agent", status: "completed" });

  const firstResult = indexOfFrame(h, (f) => f.type === "delta" && f.update.type === "session.execution.succeeded" && f.update.data.sessionID === "ses_f1485cda4ffeXuc6GjAU9vDiRb");
  const childEnded = indexOfFrame(h, (f) => f.type === "background" && f.event.type === "ended" && f.event.kind === "agent");
  const runDone = indexOfFrame(h, (f) => f.type === "runDone");
  assert.ok(firstResult >= 0 && firstResult < childEnded, "the parent's first result arrives while the subagent still runs");
  assert.ok(runDone > childEnded, "the run stayed open until the subagent ended");
  const wake = h.all.filter((f) => f.type === "delta" && f.update.type === "session.execution.succeeded" && f.update.data.sessionID === "ses_f1485cda4ffeXuc6GjAU9vDiRb");
  assert.equal(wake.length, 2);
  assert.ok(runDone > h.all.indexOf(wake[1]), "the wake turn's result ends the run");
  assert.ok(h.all.some((f) => f.type === "log" && f.event.type === "runHeld"));
  assert.deepEqual(replay.calls.failures, []);
});

test("nested background transcript: the root run is held for a grandchild, then ends once when no wake turn follows it", async () => {
  const { replay, h } = replayHost("nested_background");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  const frames = backgroundFrames(h).map((f) => [f.event.type, f.event.kind, f.event.background]);
  assert.deepEqual(frames.filter((f) => f[1] === "agent"), [["started", "agent", false], ["started", "agent", true], ["ended", "agent", undefined], ["ended", "agent", undefined]]);
  const grandchild = "ses_f08c0655dffeXJa3hcJz6wum9I";
  const ended = indexOfFrame(h, (f) => f.type === "background" && f.event.type === "ended" && f.event.id === grandchild);
  const shellEnded = indexOfFrame(h, (f) => f.type === "background" && f.event.type === "ended" && f.event.kind === "shell");
  assert.ok(shellEnded >= 0, "the grandchild's shell is reported too");
  assert.ok(indexOfFrame(h, (f) => f.type === "runDone") > ended, "held until the grandchild ended");
  const rootResults = h.all.filter((f) => f.type === "delta" && f.update.type === "session.execution.succeeded" && f.update.data.sessionID === "ses_f08c08821ffefFAD3sMjkyP0Fd");
  assert.equal(rootResults.length, 1, "the root never got a wake turn; the grace ended the run");
  assert.deepEqual(replay.calls.failures, []);
});

test("subagent transcript: a foreground subagent is reported and ended before the run ends, with no hold", async () => {
  const { replay, h } = replayHost("subagent");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "completed");
  const frames = backgroundFrames(h);
  assert.deepEqual(frames.map((f) => [f.event.type, f.event.kind]), [["started", "agent"], ["ended", "agent"]]);
  assert.equal(frames[0].event.toolCallId, "call_function_okrf3eahp2li_1");
  assert.equal(frames[0].event.background, false);
  assert.equal(frames[0].event.description, "List files in current directory");
  assert.equal(h.all.some((f) => f.type === "log" && f.event.type === "runHeld"), false);
  assert.ok(indexOfFrame(h, (f) => f.type === "runDone") > indexOfFrame(h, (f) => f.type === "background" && f.event.type === "ended"));
});

function childScript(events, { background = true } = {}) {
  events.push(ev("session.tool.input.started", { sessionID: "ses_x", id: "call_a", name: "subagent" }));
  events.push(ev("session.tool.called", { sessionID: "ses_x", id: "call_a", input: { agent: "general", description: "Child", prompt: "p", background } }));
  events.push(ev("session.created", { sessionID: "ses_c", parentID: "ses_x", title: "Child", agent: "general" }));
  events.push(ev("session.tool.progress", { sessionID: "ses_x", id: "call_a", metadata: { sessionID: "ses_c", status: "running" } }));
  events.push(ev("session.execution.started", { sessionID: "ses_c" }));
}

test("a failed or interrupted first result is never held, even with a subagent running", async () => {
  for (const [type, extra, reason] of [["session.execution.failed", { error: { type: "unknown", message: "boom" } }, "failed"], ["session.execution.interrupted", { reason: "user" }, "cancelled"]]) {
    const { client, events } = fake();
    const h = boot({ client });
    await handshake(h);
    await open(h);
    await answeredRun(h);
    childScript(events);
    events.push(ev(type, { sessionID: "ses_x", ...extra }));
    const done = await h.next((f) => f.type === "runDone");
    assert.equal(done.result.reason, reason);
  }
});

test("a held run still ends once the subagent ends and no wake turn comes (grace), with the first result", async () => {
  const { client, events } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  childScript(events);
  events.push(ev("session.execution.succeeded", { sessionID: "ses_x" }));
  await h.next((f) => f.type === "log" && f.event.type === "runHeld");
  await settle();
  assert.equal(runs(h, 3).length, 0, "still open while the subagent runs");
  events.push(ev("session.execution.succeeded", { sessionID: "ses_c" }));
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1);
});

test("cancel during a hold interrupts the session and its running subagents and ends the run once as cancelled", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  childScript(events);
  events.push(ev("session.execution.succeeded", { sessionID: "ses_x" }));
  await h.next((f) => f.type === "log" && f.event.type === "runHeld");
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((f) => f.id === 4)).result, { cancelled: true });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "cancelled");
  assert.deepEqual(calls.interrupts.map((i) => i.sessionID).sort(), ["ses_c", "ses_x"]);
  await settle();
  assert.equal(runs(h, 3).length, 1);
});

test("a subagent's permission ask is a frame on the run and is answered on the subagent's own session", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  childScript(events);
  events.push(ev("permission.asked", { id: "per_9", sessionID: "ses_c", action: "shell", resources: ["ls"] }));
  const ask = await h.next((f) => f.type === "permission");
  assert.equal(ask.sendId, 3);
  assert.deepEqual(ask.input, { action: "shell", resources: ["ls"] });
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  await h.next((f) => f.id === 4);
  assert.deepEqual(calls.replies, [{ sessionID: "ses_c", requestID: "per_9", decision: "once" }]);
});

const formEvent = (fields, extra = {}, sessionID = "ses_x") => ev("form.created", { form: { id: "frm_1", sessionID, title: "Questions", metadata: { kind: "question", tool: { messageID: "msg_a", id: "call_q" } }, fields, ...extra } });

test("question transcript: the form becomes a question frame with the tool's own input; the answer reaches session.form.reply by field key", async () => {
  const { replay, h } = replayHost("question");
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const ask = await h.next((f) => f.type === "question");
  assert.equal(ask.sendId, 3);
  assert.match(ask.requestId, /^q-/);
  assert.equal(ask.tool, "question");
  assert.deepEqual(ask.input, { questions: [{ question: "Which color do you prefer?", header: "Color preference", options: [{ label: "Red", description: "Choose the color red" }, { label: "Blue", description: "Choose the color blue" }] }] });
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { answers: { "Which color do you prefer?": "blue" } } });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.calls.forms, [{ sessionID: "ses_f14865e15ffewHnFwxdBlN6Eqc", formID: "frm_0eb79ab35001fkvFECSh3wYNVD", answer: { q0: "blue" } }]);
  assert.deepEqual(replay.calls.failures, []);
  assert.deepEqual(replay.remaining(), []);
  h.send({ id: 5, type: "respond", requestId: ask.requestId, decision: { answers: {} } });
  assert.equal((await h.next((f) => f.id === 5)).ok, true, "a second respond is a late one");
  await h.next((f) => f.type === "log" && f.event.type === "lateRespond");
});

test("question: skipping cancels the form; the run goes on", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  events.push(ev("session.tool.called", { sessionID: "ses_x", id: "call_q", input: { questions: [{ question: "Pick?", header: "H", options: [{ label: "a", description: "" }] }] } }));
  events.push(formEvent([{ key: "q0", title: "H", description: "Pick?", type: "string", options: [{ value: "a", label: "a" }] }]));
  const ask = await h.next((f) => f.type === "question");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "deny" } });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  assert.deepEqual(calls.cancels, [{ sessionID: "ses_x", formID: "frm_1" }]);
  assert.deepEqual(calls.forms, []);
});

test("question: several selections are joined by commas and become the field's list; a missing answer is refused and can be retried", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  events.push(ev("session.tool.called", { sessionID: "ses_x", id: "call_q", input: { questions: [{ question: "Which?", header: "A", multiple: true, options: [{ label: "x", description: "" }, { label: "y", description: "" }] }, { question: "Why?", header: "B", options: [] }] } }));
  events.push(formEvent([
    { key: "q0", title: "A", description: "Which?", type: "multiselect", options: [{ value: "x", label: "x" }, { value: "y", label: "y" }], custom: true },
    { key: "q1", title: "B", description: "Why?", type: "string", required: true },
  ]));
  const ask = await h.next((f) => f.type === "question");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { answers: { "Which?": "x, y" } } });
  const refused = await h.next((f) => f.id === 4);
  assert.equal(refused.ok, false);
  assert.equal(refused.error.cause.kind, "invalid_request");
  assert.deepEqual(calls.forms, []);
  h.send({ id: 5, type: "respond", requestId: ask.requestId, decision: { answers: { "Which?": "x, y", "Why?": "because" } } });
  assert.equal((await h.next((f) => f.id === 5)).ok, true);
  assert.deepEqual(calls.forms, [{ sessionID: "ses_x", formID: "frm_1", answer: { q0: ["x", "y"], q1: "because" } }]);
});

test("question: a form of a kind the host does not understand is refused, never left open, and logged", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  events.push(formEvent([{ key: "k", type: "string" }], { metadata: { kind: "integration" } }));
  const log = await h.next((f) => f.type === "log" && f.event.type === "unsupportedForm");
  assert.equal(log.event.kind, "integration");
  await settle();
  assert.equal(h.all.some((f) => f.type === "question"), false);
  assert.equal(calls.cancels.length, 1);
  assert.equal(calls.cancels[0].formID, "frm_1");
});

test("question: a form asked from a subagent reaches the run and is answered on the subagent's session", async () => {
  const { client, events, calls } = fake();
  const h = boot({ client });
  await handshake(h);
  await open(h);
  await answeredRun(h);
  childScript(events);
  events.push(ev("session.tool.called", { sessionID: "ses_c", id: "call_q", input: { questions: [{ question: "Pick?", header: "H", options: [{ label: "a", description: "" }] }] } }));
  events.push(formEvent([{ key: "q0", title: "H", description: "Pick?", type: "string", options: [{ value: "a", label: "a" }] }], {}, "ses_c"));
  const ask = await h.next((f) => f.type === "question");
  assert.equal(ask.sendId, 3);
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { answers: { "Pick?": "a" } } });
  await h.next((f) => f.id === 4);
  assert.deepEqual(calls.forms, [{ sessionID: "ses_c", formID: "frm_1", answer: { q0: "a" } }]);
});

test("a send is refused for an unknown session, a second send during a run, and a message that is not text", async () => {
  const { client } = scripted([]);
  const h = boot({ client });
  await handshake(h);
  h.send({ id: 3, type: "send", sessionId: "nope", message: "hi" });
  const unknown = await h.next((f) => f.id === 3);
  assert.equal(unknown.ok, false);
  assert.equal(unknown.error.cause.name, "UnknownSession");
  assert.equal(unknown.error.cause.kind, "invalid_request");

  await open(h);
  h.send({ id: 4, type: "send", sessionId: "s", message: { attachments: [] } });
  const notText = await h.next((f) => f.id === 4);
  assert.equal(notText.error.cause.name, "InvalidMessage");
  assert.equal(notText.error.cause.kind, "invalid_request");

  h.send({ id: 5, type: "send", sessionId: "s", message: "first" });
  assert.equal((await h.next((f) => f.id === 5)).ok, true);
  h.send({ id: 6, type: "send", sessionId: "s", message: "second" });
  const busy = await h.next((f) => f.id === 6);
  assert.equal(busy.ok, false);
  assert.equal(busy.error.cause.name, "RunInProgress");
  assert.equal(busy.error.cause.kind, "invalid_request");
  assert.equal(runs(h, 6).length, 0, "the refused send never starts a run");
});

test("a request type the host does not know is refused as UnsupportedRequest", async () => {
  const { client } = scripted([]);
  const h = boot({ client });
  await handshake(h);
  await open(h);
  h.send({ id: 3, type: "frobnicate", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.name, "UnsupportedRequest");
});
