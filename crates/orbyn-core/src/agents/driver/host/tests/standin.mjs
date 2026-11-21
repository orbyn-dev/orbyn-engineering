// A scripted stand-in for orbyn-engine-host, for the Rust tests of the host driver. It speaks the
// protocol of docs/engine-host-protocol.md and misbehaves on purpose, in ways the real `fake`
// family does not (a late hello, another protocol, a second runDone, an oversized frame, a stall).
// Behaviour comes from environment variables (the host's spawn environment) and from tags in the
// prompt. Every request it receives is appended to $STANDIN_LOG as one JSON line.
import { appendFileSync, existsSync } from "node:fs";
import { createInterface } from "node:readline";

const env = process.env;
const log = (value) => env.STANDIN_LOG && appendFileSync(env.STANDIN_LOG, `${JSON.stringify(value)}\n`);
const write = (frame) => process.stdout.write(`${JSON.stringify(frame)}\n`);
const capabilities = (env.STANDIN_CAPS ?? "steer,stopTask,background").split(",").filter(Boolean);

const hello = () =>
  write({ type: "hello", protocol: Number(env.STANDIN_PROTOCOL ?? 1), host: "9.9.9", family: env.STANDIN_FAMILY ?? "fake", sdk: { name: "standin", version: "0" }, capabilities });

// What the spawn environment and argv held, as booleans: the key's value is never written down.
const key = env.CURSOR_API_KEY ?? "";
log({ type: "spawn", keyInEnv: key !== "", keyInArgv: key !== "" && process.argv.some((arg) => arg.includes(key)) });
if (env.STANDIN_LEAK_KEY) process.stderr.write(`standin: sign-in failed for ${key}\n`);

// STANDIN_NEEDS_FILE: the host cannot start until that file exists (a fault that is fixed later).
const broken = env.STANDIN_NEEDS_FILE && !existsSync(env.STANDIN_NEEDS_FILE);
if (env.STANDIN_EXIT_BEFORE_HELLO || broken) {
  process.stderr.write("standin: exiting before hello\nANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnop\n");
  process.exit(Number(env.STANDIN_EXIT_BEFORE_HELLO ?? 1));
}
setTimeout(hello, Number(env.STANDIN_HELLO_DELAY_MS ?? 0));

const runs = new Map(); // sendId -> {cancelled}
const pending = new Map(); // requestId -> {sendId}
let stalled = false;

const done = (sendId, result = { reason: "completed", threadDisposition: "reusable" }, extra = {}) =>
  write({ type: "runDone", sendId, ok: true, result, ...extra });

async function play(sendId, text) {
  if (text.includes("[crash]")) process.exit(3);
  if (text.includes("[stall]")) {
    stalled = true; // never answers anything again, ping included
    return;
  }
  if (text.includes("[hang]")) return;
  if (text.includes("[big]")) {
    process.stdout.write(`${JSON.stringify({ type: "delta", sendId, update: { text: "x".repeat(9 * 1024 * 1024) } })}\n`);
    return;
  }
  if (text.includes("[exactly]")) {
    // A frame whose line is exactly the 8 MiB cap: allowed.
    const base = JSON.stringify({ type: "delta", sendId, update: { text: "" } });
    const pad = 8 * 1024 * 1024 - Buffer.byteLength(base);
    process.stdout.write(`${JSON.stringify({ type: "delta", sendId, update: { text: "y".repeat(pad) } })}\n`);
    return done(sendId);
  }
  if (text.includes("[malformed")) {
    const n = Number(/\[malformed:(\d+)\]/.exec(text)?.[1] ?? 3);
    for (let i = 0; i < n; i++) process.stdout.write("{not json\n");
    write({ type: "delta", sendId, update: { text: "after" } });
    return done(sendId);
  }
  if (text.includes("[dup]")) {
    done(sendId);
    done(sendId, { reason: "failed" });
    return;
  }
  if (text.includes("[bg]")) {
    write({ type: "background", sessionId: runs.get(sendId).session, event: { type: "started", id: "task-1", kind: "agent", toolCallId: "call-1" } });
    write({ type: "delta", sendId, update: { text: "started a task" } });
    return done(sendId);
  }
  if (text.includes("[perm")) {
    // [perm]: an edit inside the work folder. [perm-opts]: with ACP-style options. [perm-own]: an Orbyn tool.
    // [perm-crash]: the host dies with the ask open.
    const tool = text.includes("[perm-own]") ? "mcp__orbyn_engineering__orbyn_thread_list" : "Edit";
    const frame = { type: "permission", sendId, requestId: `p-${sendId}`, tool, input: { file_path: "/workspace/repo/app/a.txt", old_string: "x", new_string: "y" } };
    if (text.includes("[perm-opts]")) frame.options = [{ kind: "allow_once", name: "Allow once" }, { kind: "reject_once", name: "Reject" }];
    write(frame);
    pending.set(frame.requestId, { sendId });
    if (text.includes("[perm-crash]")) setTimeout(() => process.exit(3), 100);
    return;
  }
  if (text.includes("[question")) {
    // [question]: nothing to show. [question-1]: one multi-select question.
    const questions = text.includes("[question-1]")
      ? [{ question: "Which parts?", header: "Parts", multiSelect: true, options: [{ label: "A", description: "first" }, { label: "B", description: "second" }] }]
      : [];
    write({ type: "question", sendId, requestId: `q-${sendId}`, tool: "AskUserQuestion", input: { questions } });
    pending.set(`q-${sendId}`, { sendId });
    return;
  }
  if (text.includes("[leftover")) {
    // The engine ran a turn for its own leftovers first: the prompt is queued, the old turn speaks, then the prompt starts.
    const uuid = runs.get(sendId).uuid;
    const lifecycle = (state) => write({ type: "delta", sendId, update: { type: "command_lifecycle", command_uuid: uuid, state } });
    lifecycle("queued");
    write({ type: "delta", sendId, update: { text: "the old agent finished" } });
    lifecycle("started");
    if (text.includes("[leftover-answer]")) write({ type: "delta", sendId, update: { text: "here is your answer" } });
    return done(sendId);
  }
  if (text.includes("[limit]")) {
    return write({
      type: "runDone", sendId, ok: false,
      error: { method: "send", cause: { name: "RateLimit", message: "Limit reached", kind: "rate_limit" } },
      result: { reason: "failed", threadDisposition: "reusable", retry: { why: "rate_limit", resetsAt: 1900000000 } },
    });
  }
  if (text.includes("[maxturns]")) return done(sendId, { reason: "maxTurns", threadDisposition: "reusable" });
  if (text.includes("[lost]")) {
    return write({ type: "runDone", sendId, ok: false, error: { method: "send", cause: { name: "E", message: "gone", kind: "session_lost" } } });
  }
  write({ type: "delta", sendId, update: { text: `echo: ${text}` } });
  done(sendId);
}

const handlers = {
  init: (r) => {
    if (r.protocol !== Number(env.STANDIN_PROTOCOL ?? 1)) throw Object.assign(new Error("protocol"), { name: "ProtocolMismatch" });
    return { ready: true };
  },
  open: (r) => {
    if (r.confinement && env.STANDIN_UNSUPPORTED) throw Object.assign(new Error("cannot confine"), { kind: "unsupported" });
    if (r.operation === "resume" && r.engineSessionId === "gone") throw Object.assign(new Error("no such session"), { kind: "session_lost" });
    return { engineSessionId: r.engineSessionId ?? `eng-${r.sessionId}` };
  },
  send: (r) => {
    runs.set(r.id, { cancelled: false, text: String(r.message), session: r.sessionId, uuid: r.messageId });
    setImmediate(() => {
      write({ type: "session", sendId: r.id, engineSessionId: `eng-${r.sessionId}`, turnId: `turn-${r.id}` });
      void play(r.id, String(r.message));
    });
    return { runId: `run-${r.id}`, engineSessionId: `eng-${r.sessionId}`, turnId: `turn-${r.id}`, messageId: r.messageId };
  },
  respond: (r) => {
    const held = pending.get(r.requestId);
    if (!held) {
      write({ type: "log", level: "warn", event: { type: "lateRespond", requestId: r.requestId } });
      return null;
    }
    pending.delete(r.requestId);
    write({ type: "delta", sendId: held.sendId, update: { text: `decision ${JSON.stringify(r.decision)}` } });
    done(held.sendId);
    return null;
  },
  cancel: (r) => {
    const run = runs.get(r.sendId);
    if (!run) return { cancelled: false };
    // [failcancel]: like the Claude SDK's interrupt, which ends the turn as a failure.
    if (run.text.includes("[failcancel]")) {
      write({ type: "runDone", sendId: r.sendId, ok: false, error: { method: "send", cause: { name: "E", message: "interrupted", kind: "engine" } }, result: { reason: "failed", threadDisposition: "reusable" } });
      return { cancelled: true };
    }
    done(r.sendId, { reason: "cancelled", threadDisposition: "reusable" });
    return { cancelled: true };
  },
  steer: (r) => ({ outcome: String(r.text).includes("reject") ? "rejected" : "accepted", messageId: r.messageId }),
  stopTask: () => ({ stopped: true }),
  closeSession: () => null,
  ping: () => null,
  close: () => {
    setImmediate(() => process.exit(0));
    return null;
  },
};

createInterface({ input: process.stdin }).on("line", (line) => {
  if (env.STANDIN_LOG) appendFileSync(`${env.STANDIN_LOG}.raw`, `${line}\n`);
  if (stalled) return;
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    return;
  }
  log({ type: request.type, requestId: request.requestId, decision: request.decision, sendId: request.sendId, taskId: request.taskId, text: request.text, messageId: request.messageId, confinement: Boolean(request.confinement) });
  const handler = handlers[request.type];
  try {
    if (!handler) throw Object.assign(new Error(`unsupported ${request.type}`), { name: "UnsupportedRequest" });
    write({ type: "response", id: request.id, ok: true, result: handler(request) ?? null });
  } catch (cause) {
    write({ type: "response", id: request.id, ok: false, error: { method: request.type, cause: { name: cause.name ?? "Error", message: cause.message, ...(cause.kind ? { kind: cause.kind } : {}) } } });
  }
});
process.stdin.on("close", () => process.exit(0));
