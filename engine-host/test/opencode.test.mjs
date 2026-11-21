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

function boot({ client, launch, sdkClient }) {
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
  assert.deepEqual(hello.capabilities, ["steer", "fork"]);
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
