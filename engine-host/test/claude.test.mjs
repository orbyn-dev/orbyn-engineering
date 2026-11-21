import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { claudeFamily } from "../src/families/claude.mjs";
import { runHost } from "../src/host.mjs";
import { replaySdk } from "./replay.mjs";

const fixture = (n) => fileURLToPath(new URL(`./fixtures/claude/${n}.ndjson`, import.meta.url));

function boot(file) {
  const replay = replaySdk(fixture(file));
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    frames.push(JSON.parse(line));
    for (const w of [...waiters]) w();
  });
  runHost({
    family: claudeFamily({ loadSdk: async () => ({ query: replay.query, forkSession: async () => ({ sessionId: "forked" }) }) }),
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
  return { replay, next, send, input, file: fixture(file) };
}

async function open(h, extra = {}) {
  const { readFileSync } = await import("node:fs");
  const o = readFileSync(h.file, "utf8").split("\n").map((l) => l && JSON.parse(l)).find((l) => l?.frame?.type === "query.open").frame.options;
  const sdk = { model: o.model, permissionMode: o.permissionMode, ...(o.allowDangerouslySkipPermissions ? { allowDangerouslySkipPermissions: true } : {}) };
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((f) => f.id === 1);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/tmp/x", options: { sdk, ...extra } });
  assert.equal((await h.next((f) => f.id === 2)).ok, true);
}

test("multi_turn: two prompts, two runDone, session id reported", async () => {
  const h = boot("multi_turn");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: "Respond with exactly: first fixture turn complete" });
  const session = await h.next((f) => f.type === "session");
  assert.equal(session.engineSessionId, "01899f29-45f7-43c3-b963-0f9181ed013f");
  const done1 = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done1.result.reason, "completed");
  h.send({ id: 4, type: "send", sessionId: "s", message: "Respond with exactly: second fixture turn complete" });
  const done2 = await h.next((f) => f.type === "runDone" && f.sendId === 4);
  assert.equal(done2.result.reason, "completed");
  assert.equal(h.replay.calls.opens.length, 1, "one query for the session");
  assert.equal(typeof h.replay.calls.opens[0].spawnClaudeCodeProcess, "function");
});

test("an allow reaches the SDK once-only: no updatedPermissions", async () => {
  const h = boot("tool_call_read_only_on_request");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: (await promptOf("tool_call_read_only_on_request")) });
  const ask = await h.next((f) => f.type === "permission");
  assert.equal(ask.tool, "Bash");
  assert.ok(ask.input.command.includes("printf"));
  assert.equal("suggestions" in ask, false);
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "completed");
  const res = h.replay.calls.permissionResults[0];
  assert.equal(res.behavior, "allow");
  assert.equal("updatedPermissions" in res, false);
});

test("a deny reaches the SDK with the person's message", async () => {
  const h = boot("tool_call_denied_write");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: await promptOf("tool_call_denied_write") });
  const ask = await h.next((f) => f.type === "permission");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "deny", message: "No writes here." } });
  await h.next((f) => f.type === "runDone");
  assert.deepEqual(h.replay.calls.permissionResults[0], { behavior: "deny", message: "No writes here." });
});

test("cancel interrupts the SDK and ends the run once as cancelled", async () => {
  const h = boot("claude_background_task_interrupt");
  await open(h, { cancelGraceMs: 20 });
  h.send({ id: 3, type: "send", sessionId: "s", message: await promptOf("claude_background_task_interrupt") });
  const bg = await h.next((f) => f.type === "background");
  assert.equal(bg.event.kind, "shell");
  assert.ok(bg.event.toolCallId);
  h.send({ id: 4, type: "cancel", sendId: 3 });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "cancelled");
  assert.equal(h.replay.calls.interrupts, 1);
});

async function promptOf(name) {
  const { readFileSync } = await import("node:fs");
  const first = readFileSync(fixture(name), "utf8").split("\n").map((l) => l && JSON.parse(l)).find((l) => l?.frame?.type === "prompt.offer");
  return first.frame.message.message.content;
}

test("an authentication failure result ends the run with kind auth", async () => {
  const { AsyncQueue } = await import("../src/asyncQueue.mjs");
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  createInterface({ input: output }).on("line", (l) => frames.push(JSON.parse(l)));
  const query = ({ prompt }) => {
    const out = new AsyncQueue();
    (async () => {
      for await (const _ of prompt) {
        out.push({ type: "result", subtype: "success", is_error: true, result: "Failed to authenticate: OAuth session expired and could not be refreshed", session_id: "x" });
      }
    })();
    return out[Symbol.asyncIterator] ? { [Symbol.asyncIterator]: () => out[Symbol.asyncIterator](), close() {}, interrupt: async () => {} } : null;
  };
  runHost({ family: claudeFamily({ loadSdk: async () => ({ query }) }), input, output });
  const send = (f) => input.write(`${JSON.stringify(f)}\n`);
  send({ id: 1, type: "init", protocol: 1 });
  send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/tmp", options: { sdk: {} } });
  send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await new Promise((resolve) => {
    const t = setInterval(() => frames.some((f) => f.type === "runDone") && (clearInterval(t), resolve()), 5);
  });
  const done = frames.find((f) => f.type === "runDone");
  assert.equal(done.ok, false);
  assert.equal(done.error.cause.kind, "auth");
});

test("models, usage, stopTask and background call the SDK query", async () => {
  const { AsyncQueue } = await import("../src/asyncQueue.mjs");
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  createInterface({ input: output }).on("line", (l) => frames.push(JSON.parse(l)));
  const calls = [];
  const query = ({ prompt }) => {
    const out = new AsyncQueue();
    return {
      [Symbol.asyncIterator]: () => out[Symbol.asyncIterator](),
      close() { calls.push("close"); },
      interrupt: async () => {},
      supportedModels: async () => [{ value: "m1", supportedEffortLevels: ["low"] }],
      getContextUsage: async (o) => ({ total_tokens: 5, opts: o }),
      stopTask: async (id) => calls.push(`stop:${id}`),
      backgroundTasks: async (id) => (calls.push(`bg:${id}`), true),
    };
  };
  runHost({ family: claudeFamily({ loadSdk: async () => ({ query }) }), input, output });
  const send = (f) => input.write(`${JSON.stringify(f)}\n`);
  const reply = (id) => new Promise((resolve) => {
    const t = setInterval(() => { const f = frames.find((x) => x.id === id); if (f) { clearInterval(t); resolve(f); } }, 5);
  });
  send({ id: 1, type: "init", protocol: 1 });
  send({ id: 2, type: "models", claudePath: "/bin/claude" });
  assert.equal((await reply(2)).result[0].value, "m1");
  send({ id: 3, type: "open", operation: "create", sessionId: "s", cwd: "/tmp", options: { sdk: {} } });
  await reply(3);
  send({ id: 4, type: "usage", sessionId: "s" });
  assert.equal((await reply(4)).ok, false, "no live query before the first send");
  send({ id: 5, type: "send", sessionId: "s", message: "hi" });
  await reply(5);
  send({ id: 6, type: "usage", sessionId: "s" });
  assert.deepEqual((await reply(6)).result, { total_tokens: 5, opts: { detail: "summary" } });
  send({ id: 7, type: "stopTask", sessionId: "s", taskId: "t1" });
  assert.equal((await reply(7)).result.stopped, true);
  send({ id: 8, type: "background", sendId: 5, toolCallId: "tu1" });
  assert.equal((await reply(8)).result.backgrounded, true);
  assert.deepEqual(calls.filter((c) => c !== "close"), ["stop:t1", "bg:tu1"]);
  assert.ok(calls.includes("close"), "the models probe is closed");
});

test("AskUserQuestion is a question frame; answers come back in updatedInput", async () => {
  const { AsyncQueue } = await import("../src/asyncQueue.mjs");
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  createInterface({ input: output }).on("line", (l) => frames.push(JSON.parse(l)));
  let toolResult;
  const query = ({ prompt, options }) => {
    const out = new AsyncQueue();
    (async () => {
      for await (const _ of prompt) {
        toolResult = await options.canUseTool("AskUserQuestion", { questions: [{ question: "Which?", header: "h", options: [], multiSelect: false }] }, {});
        out.push({ type: "result", subtype: "success", is_error: false, result: "ok", session_id: "x" });
      }
    })();
    return { [Symbol.asyncIterator]: () => out[Symbol.asyncIterator](), close() {}, interrupt: async () => {} };
  };
  runHost({ family: claudeFamily({ loadSdk: async () => ({ query }) }), input, output });
  const send = (f) => input.write(`${JSON.stringify(f)}\n`);
  const wait = (pred) => new Promise((resolve) => {
    const t = setInterval(() => { const f = frames.find(pred); if (f) { clearInterval(t); resolve(f); } }, 5);
  });
  send({ id: 1, type: "init", protocol: 1 });
  send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/tmp", options: { sdk: {} } });
  send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  const q = await wait((f) => f.type === "question");
  assert.equal(q.tool, "AskUserQuestion");
  assert.equal(frames.some((f) => f.type === "permission"), false);
  send({ id: 4, type: "respond", requestId: q.requestId, decision: { answers: { "Which?": "A" } } });
  await wait((f) => f.type === "runDone");
  assert.equal(toolResult.behavior, "allow");
  assert.deepEqual(toolResult.updatedInput.answers, { "Which?": "A" });
  assert.equal("updatedPermissions" in toolResult, false);
});
