import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { decodeFrame, serializeError } from "../src/frame.mjs";

const main = fileURLToPath(new URL("../src/main.mjs", import.meta.url));

function start() {
  const child = spawn(process.execPath, [main, "fake"], { stdio: ["pipe", "pipe", "pipe"] });
  const frames = [];
  const waiters = [];
  createInterface({ input: child.stdout }).on("line", (line) => {
    let frame;
    try {
      frame = decodeFrame(line);
    } catch {
      frame = { type: "unparsed", line };
    }
    frames.push(frame);
    for (const w of [...waiters]) w();
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
  const send = (frame) => child.stdin.write(`${JSON.stringify(frame)}\n`);
  const exited = new Promise((resolve) => child.on("exit", (code) => resolve(code)));
  return { child, send, next, exited };
}

async function ready(h) {
  const hello = await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: hello.protocol });
  await h.next((f) => f.type === "response" && f.id === 1);
  return hello;
}

test("hello announces protocol, family and capabilities", async () => {
  const h = start();
  const hello = await ready(h);
  assert.equal(hello.protocol, 1);
  assert.equal(hello.family, "fake");
  assert.ok(hello.capabilities.includes("steer"));
  h.send({ id: 2, type: "close" });
  assert.equal(await h.exited, 0);
});

test("a protocol mismatch is refused and requests before init fail", async () => {
  const h = start();
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "open" });
  const early = await h.next((f) => f.id === 1);
  assert.equal(early.ok, false);
  assert.equal(early.error.cause.name, "NotInitialised");
  h.send({ id: 2, type: "init", protocol: 99 });
  const refused = await h.next((f) => f.id === 2);
  assert.equal(refused.error.cause.name, "ProtocolMismatch");
  h.child.kill();
});

test("a turn streams a delta then exactly one runDone", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s" });
  const opened = await h.next((f) => f.id === 2);
  assert.match(opened.result.engineSessionId, /^fake-/);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  const accepted = await h.next((f) => f.type === "response" && f.id === 3);
  assert.equal(accepted.ok, true);
  const delta = await h.next((f) => f.type === "delta" && f.sendId === 3);
  assert.equal(delta.update.text, "echo: hi");
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done.result.reason, "completed");
  assert.equal(done.result.threadDisposition, "reusable");
  h.child.kill();
});

test("permission blocks until respond; a late respond is only logged", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "send", sessionId: "s", message: "[permission]" });
  const ask = await h.next((f) => f.type === "permission");
  assert.equal(ask.tool, "Edit");
  h.send({ id: 3, type: "respond", requestId: ask.requestId, decision: "allowed" });
  const delta = await h.next((f) => f.type === "delta");
  assert.equal(delta.update.text, "permission allowed");
  await h.next((f) => f.type === "runDone");
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: "allowed" });
  const log = await h.next((f) => f.type === "log" && f.event.type === "lateRespond");
  assert.equal(log.event.requestId, ask.requestId);
  h.child.kill();
});

test("cancel while a permission is pending refuses it and ends the run once", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "send", sessionId: "s", message: "[permission]" });
  await h.next((f) => f.type === "permission");
  h.send({ id: 3, type: "cancel", sendId: 2 });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 2);
  assert.equal(done.result.reason, "cancelled");
  const reply = await h.next((f) => f.type === "response" && f.id === 3);
  assert.equal(reply.result.cancelled, true);
  h.child.kill();
});

test("a failing run carries a cause chain capped at depth 8", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "send", sessionId: "s", message: "[fail]" });
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.ok, false);
  let depth = 0;
  for (let c = done.error.cause; c.cause; c = c.cause) depth++;
  assert.equal(depth, 8);
  h.child.kill();
});

test("a malformed line from the host is unparsed on the wire; a malformed request is logged and dropped", async () => {
  const h = start();
  await ready(h);
  h.child.stdin.write("{broken\n");
  const log = await h.next((f) => f.type === "log" && f.event.type === "malformedFrame");
  assert.equal(log.level, "warn");
  h.send({ id: 2, type: "ping" });
  assert.equal((await h.next((f) => f.id === 2)).ok, true);
  h.child.kill();
});

test("a crash mid-run exits the process with a code", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "send", sessionId: "s", message: "[crash]" });
  assert.equal(await h.exited, 3);
});

test("closing stdin exits the host", async () => {
  const h = start();
  await ready(h);
  h.child.stdin.end();
  assert.equal(await h.exited, 0);
});

test("unsupported requests fail with UnsupportedRequest", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "rollback", sessionId: "s", toTurn: 1 });
  const r = await h.next((f) => f.id === 2);
  assert.equal(r.error.cause.name, "UnsupportedRequest");
  h.child.kill();
});

test("serializeError survives circular causes and throwing getters", () => {
  const a = new Error("a");
  a.cause = a;
  assert.equal(serializeError(a).cause.message, "[Circular error cause]");
  const hostile = {
    get message() {
      throw new Error("boom");
    },
  };
  assert.equal(serializeError(hostile).name, "Error");
});

test("send reports the session and turn ids; steer echoes messageId; probe lists refusals", async () => {
  const h = start();
  await ready(h);
  h.send({ id: 2, type: "send", sessionId: "s", message: "hi" });
  const session = await h.next((f) => f.type === "session");
  assert.equal(session.turnId, "turn-2");
  assert.equal(session.engineSessionId, "fake-s");
  await h.next((f) => f.type === "runDone");
  h.send({ id: 3, type: "steer", sendId: 2, messageId: "m1", text: "x" });
  assert.equal((await h.next((f) => f.id === 3)).result.messageId, "m1");
  h.send({ id: 4, type: "probe", sessionId: "s", kind: "confinement" });
  assert.deepEqual((await h.next((f) => f.id === 4)).result.allowed, []);
  h.child.kill();
});
