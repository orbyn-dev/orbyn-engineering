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
