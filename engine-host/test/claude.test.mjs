import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { claudeFamily } from "../src/families/claude.mjs";
import { runHost } from "../src/host.mjs";
import { replaySdk } from "./replay.mjs";

const fixture = (n) => fileURLToPath(new URL(`./fixtures/claude/${n}.ndjson`, import.meta.url));

function boot(file, { history = [], subagentHistory = [], withSessionMessages = true, withSubagentMessages = true, query: sdkQuery } = {}) {
  const replay = replaySdk(fixture(file));
  const calls = { sessionMessages: [], subagentMessages: [], closes: 0 };
  const query =
    sdkQuery ??
    ((args) => {
      const live = replay.query(args);
      return {
        ...live,
        close() {
          calls.closes++;
          live.close?.();
        },
      };
    });
  const sdk = {
    query,
    forkSession: async () => ({ sessionId: "forked" }),
    ...(withSessionMessages
      ? {
          getSessionMessages: async (...args) => {
            calls.sessionMessages.push(args);
            return history;
          },
        }
      : {}),
    ...(withSubagentMessages
      ? {
          getSubagentMessages: async (...args) => {
            calls.subagentMessages.push(args);
            return subagentHistory;
          },
        }
      : {}),
  };
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    frames.push(JSON.parse(line));
    for (const w of [...waiters]) w();
  });
  runHost({ family: claudeFamily({ loadSdk: async () => sdk }), input, output });
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
  return { replay, calls, next, send, input, file: fixture(file) };
}

async function open(h, extra = {}, request = {}) {
  const { readFileSync } = await import("node:fs");
  const o = readFileSync(h.file, "utf8").split("\n").map((l) => l && JSON.parse(l)).find((l) => l?.frame?.type === "query.open").frame.options;
  const sdk = { model: o.model, permissionMode: o.permissionMode, ...(o.allowDangerouslySkipPermissions ? { allowDangerouslySkipPermissions: true } : {}) };
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((f) => f.id === 1);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/tmp/x", options: { sdk, ...extra }, ...request });
  assert.equal((await h.next((f) => f.id === 2)).ok, true);
}

const CAPABILITIES = ["steer", "fork", "models", "usage", "readHistory", "readSubagent", "rollback", "stopTask", "background"];
const NATIVE_SESSION = "01899f29-45f7-43c3-b963-0f9181ed013f";
const FIRST_PROMPT = "Respond with exactly: first fixture turn complete";

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

test("the prompt goes in under the id the core chose and the accepted answer names it", async () => {
  const h = boot("multi_turn");
  await open(h);
  const chosen = "11111111-1111-4111-8111-111111111111";
  h.send({ id: 3, type: "send", sessionId: "s", message: "Respond with exactly: first fixture turn complete", messageId: chosen });
  const accepted = await h.next((f) => f.id === 3);
  assert.equal(accepted.result.messageId, chosen);
  await h.next((f) => f.type === "runDone" && f.sendId === 3);
  // No id, or one that is not a UUID, and the host names the prompt itself.
  h.send({ id: 4, type: "send", sessionId: "s", message: "Respond with exactly: second fixture turn complete", messageId: "not a uuid" });
  const second = await h.next((f) => f.id === 4);
  assert.match(second.result.messageId, /^[0-9a-f-]{36}$/);
  assert.notEqual(second.result.messageId, "not a uuid");
  await h.next((f) => f.type === "runDone" && f.sendId === 4);
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

test("a leftover turn's result never ends the run; a held result waits for the follow-up turn", async () => {
  const h = boot("claude_background_wake_before_queued_prompt");
  await open(h, { cancelGraceMs: 20 });
  const prompts = (await import("node:fs")).readFileSync(h.file, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l))
    .filter((l) => l.frame?.type === "prompt.offer").map((l) => l.frame.message.message.content);
  // Run 1: the first result is held (a background agent is still running); the follow-up turn's result ends it.
  h.send({ id: 10, type: "send", sessionId: "s", message: prompts[0] });
  const held = await h.next((f) => f.type === "log" && f.event.type === "holdingResult");
  assert.equal(held.event.sendId, 10);
  assert.equal((await h.next((f) => f.type === "runDone" && f.sendId === 10)).result.reason, "completed");
  // Run 2 ends on its own result.
  h.send({ id: 11, type: "send", sessionId: "s", message: prompts[1] });
  assert.equal((await h.next((f) => f.type === "runDone" && f.sendId === 11)).result.reason, "completed");
  // Run 3: a leftover turn's result arrives before our prompt's command starts and is ignored.
  h.send({ id: 12, type: "send", sessionId: "s", message: prompts[2] });
  const leftover = await h.next((f) => f.type === "log" && f.event.type === "leftoverResult");
  assert.equal(leftover.event.sendId, 12);
  await h.next((f) => f.type === "log" && f.event.type === "holdingResult" && f.event.sendId === 12);
  h.send({ id: 13, type: "cancel", sendId: 12 });
  assert.equal((await h.next((f) => f.type === "runDone" && f.sendId === 12)).result.reason, "cancelled");
  assert.equal(h.replay.calls.error, undefined);
});

test("hello lists the Claude capabilities", async () => {
  const h = boot("multi_turn");
  const hello = await h.next((f) => f.type === "hello");
  assert.deepEqual(hello.capabilities, CAPABILITIES);
});

test("rollback closes the live query and resumes at the last top-level assistant message of the turn", async () => {
  const h = boot("multi_turn");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: FIRST_PROMPT });
  assert.equal((await h.next((f) => f.type === "runDone" && f.sendId === 3)).result.reason, "completed");
  assert.equal(h.replay.calls.opens.length, 1);
  assert.equal(h.calls.closes, 0);

  // A turn the host never reported, or one with no recorded head, is not rolled back.
  h.send({ id: 4, type: "rollback", sessionId: "s", toTurn: "turn-99" });
  const unknown = await h.next((f) => f.id === 4);
  assert.equal(unknown.ok, true);
  assert.deepEqual(unknown.result, { ok: false });
  assert.equal(h.calls.closes, 0, "an unknown turn closes nothing");

  h.send({ id: 5, type: "rollback", sessionId: "s", toTurn: "turn-3" });
  const rolledBack = await h.next((f) => f.id === 5);
  assert.equal(rolledBack.ok, true);
  assert.deepEqual(rolledBack.result, { ok: true });
  assert.equal(h.calls.closes, 1, "the live query was closed");

  h.send({ id: 6, type: "send", sessionId: "s", message: FIRST_PROMPT });
  assert.equal((await h.next((f) => f.id === 6)).ok, true);
  await h.next((f) => f.type === "runDone" && f.sendId === 6);
  assert.equal(h.replay.calls.opens.length, 2, "the next send builds a new query");
  assert.equal(h.replay.calls.opens[1].resume, NATIVE_SESSION);
  assert.equal(h.replay.calls.opens[1].resumeSessionAt, "d513887e-fb2d-47a1-adbb-e8678b42ad9a");
});

test("rollback during a run is refused with RunInProgress", async () => {
  const h = boot("tool_call_read_only_on_request");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: await promptOf("tool_call_read_only_on_request") });
  const ask = await h.next((f) => f.type === "permission");
  h.send({ id: 4, type: "rollback", sessionId: "s", toTurn: "turn-3" });
  const refused = await h.next((f) => f.id === 4);
  assert.equal(refused.ok, false);
  assert.equal(refused.error.cause.name, "RunInProgress");
  assert.equal(refused.error.cause.message, "a run is already open on this session");
  assert.equal(h.calls.closes, 0, "the live query stays open");
  h.send({ id: 5, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  assert.equal((await h.next((f) => f.type === "runDone")).result.reason, "completed");
});

test("a subagent's assistant message never becomes a turn head", async () => {
  const { AsyncQueue } = await import("../src/asyncQueue.mjs");
  const opens = [];
  const query = ({ prompt, options }) => {
    opens.push(options);
    const out = new AsyncQueue();
    (async () => {
      for await (const _ of prompt) {
        out.push({ type: "assistant", parent_tool_use_id: null, uuid: "top-head", session_id: "engine-sub" });
        out.push({ type: "assistant", parent_tool_use_id: "toolu_1", uuid: "sub-head", session_id: "engine-sub" });
        out.push({ type: "result", subtype: "success", is_error: false, session_id: "engine-sub" });
      }
      out.close();
    })();
    return { [Symbol.asyncIterator]: () => out[Symbol.asyncIterator](), close: () => out.close(), interrupt: async () => {} };
  };
  const h = boot("multi_turn", { query });
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hello" });
  await h.next((f) => f.type === "runDone" && f.sendId === 3);
  h.send({ id: 4, type: "rollback", sessionId: "s", toTurn: "turn-3" });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  h.send({ id: 5, type: "send", sessionId: "s", message: "again" });
  await h.next((f) => f.type === "runDone" && f.sendId === 5);
  assert.equal(opens.length, 2);
  assert.equal(opens[1].resume, "engine-sub");
  assert.equal(opens[1].resumeSessionAt, "top-head");
});

test("readHistory returns the SDK messages and passes the session cwd", async () => {
  const messages = [{ type: "user", uuid: "u1" }, { type: "assistant", uuid: "a1" }];
  const h = boot("multi_turn", { history: messages });
  await open(h, {}, { operation: "resume", engineSessionId: "engine-1" });
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(res.ok, true);
  assert.deepEqual(res.result, messages);
  assert.deepEqual(h.calls.sessionMessages, [["engine-1", { dir: "/tmp/x" }]]);
});

test("readHistory needs an engine session and a getSessionMessages function", async () => {
  const h = boot("multi_turn", { withSessionMessages: false });
  await open(h);
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const missingSession = await h.next((f) => f.id === 3);
  assert.equal(missingSession.ok, false);
  assert.equal(missingSession.error.cause.name, "NoEngineSession");
  assert.equal(missingSession.error.cause.message, "this session has no engine session yet");

  h.send({ id: 4, type: "open", operation: "resume", sessionId: "s2", engineSessionId: "engine-1", cwd: "/tmp/x", options: { sdk: {} } });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  h.send({ id: 5, type: "readHistory", sessionId: "s2" });
  const unsupported = await h.next((f) => f.id === 5);
  assert.equal(unsupported.ok, false);
  assert.equal(unsupported.error.cause.name, "UnsupportedRequest");
});

test("readHistory fails with HistoryTooLarge past the shared budget", async () => {
  const history = [{ type: "assistant", message: { content: "x".repeat(6 * 1024 * 1024 + 1024) } }];
  const h = boot("multi_turn", { history });
  await open(h, {}, { operation: "resume", engineSessionId: "engine-1" });
  h.send({ id: 3, type: "readHistory", sessionId: "s" });
  const res = await h.next((f) => f.id === 3);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.name, "HistoryTooLarge");
  assert.equal(res.error.cause.message, "This conversation is too large to read in one piece.");
  assert.equal(res.error.cause.kind, "host");
});

test("readSubagent validates the agent id and limit, passes both to the SDK and returns its messages", async () => {
  const messages = [{ type: "assistant", uuid: "s1", parent_tool_use_id: "toolu_1" }];
  const h = boot("multi_turn", { subagentHistory: messages });
  await open(h, {}, { operation: "resume", engineSessionId: "engine-1" });

  const invalid = [
    { agentId: "bad/agent" },
    { agentId: "" },
    { agentId: "a".repeat(129) },
    { agentId: "agent_1", limit: 0 },
    { agentId: "agent_1", limit: -1 },
    { agentId: "agent_1", limit: 1.5 },
    { agentId: "agent_1", limit: 5001 },
    { agentId: "agent_1", limit: "5" },
  ];
  let id = 3;
  for (const request of invalid) {
    h.send({ id, type: "readSubagent", sessionId: "s", ...request });
    const res = await h.next((f) => f.id === id);
    assert.equal(res.ok, false, JSON.stringify(request));
    assert.equal(res.error.cause.name, "InvalidRequest", JSON.stringify(request));
    assert.equal(res.error.cause.kind, "invalid_request", JSON.stringify(request));
    id++;
  }
  assert.equal(h.calls.subagentMessages.length, 0);

  h.send({ id, type: "readSubagent", sessionId: "s", agentId: "agent_1", limit: 25 });
  const withLimit = await h.next((f) => f.id === id);
  assert.equal(withLimit.ok, true);
  assert.deepEqual(withLimit.result, messages);
  assert.deepEqual(h.calls.subagentMessages[0], ["engine-1", "agent_1", { dir: "/tmp/x", limit: 25 }]);

  h.send({ id: id + 1, type: "readSubagent", sessionId: "s", agentId: "agent-1" });
  const withoutLimit = await h.next((f) => f.id === id + 1);
  assert.equal(withoutLimit.ok, true);
  assert.deepEqual(withoutLimit.result, messages);
  assert.deepEqual(h.calls.subagentMessages[1], ["engine-1", "agent-1", { dir: "/tmp/x" }]);
});

test("readSubagent fails with HistoryTooLarge and needs an engine session", async () => {
  const h = boot("multi_turn", { subagentHistory: [{ type: "assistant", message: { content: "x".repeat(6 * 1024 * 1024 + 1024) } }] });
  await open(h);
  h.send({ id: 3, type: "readSubagent", sessionId: "s", agentId: "agent_1" });
  const noSession = await h.next((f) => f.id === 3);
  assert.equal(noSession.ok, false);
  assert.equal(noSession.error.cause.name, "NoEngineSession");

  h.send({ id: 4, type: "open", operation: "resume", sessionId: "s2", engineSessionId: "engine-1", cwd: "/tmp/x", options: { sdk: {} } });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  h.send({ id: 5, type: "readSubagent", sessionId: "s2", agentId: "agent_1", limit: 10 });
  const tooLarge = await h.next((f) => f.id === 5);
  assert.equal(tooLarge.ok, false);
  assert.equal(tooLarge.error.cause.name, "HistoryTooLarge");
  assert.equal(tooLarge.error.cause.kind, "host");
});

const HEAD = "d513887e-fb2d-47a1-adbb-e8678b42ad9a";

async function resumeAtOpen(h, extra) {
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((f) => f.id === 1);
  h.send({ id: 2, type: "open", operation: "resume", engineSessionId: NATIVE_SESSION, sessionId: "s", cwd: "/tmp/x", options: { sdk: {} }, ...extra });
  return h.next((f) => f.id === 2);
}

test("open with resumeAt passes it as resumeSessionAt on the first query", async () => {
  const seen = [];
  const query = ({ options }) => {
    seen.push(options);
    return { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true }) }), close() {} };
  };
  const h = boot("multi_turn", { query, history: [{ type: "user", uuid: "u1" }, { type: "assistant", uuid: HEAD }, { type: "assistant", uuid: "9e1d0f55-0000-4000-8000-000000000001", parent_tool_use_id: "toolu_1" }] });
  assert.equal((await resumeAtOpen(h, { resumeAt: HEAD })).ok, true);
  assert.deepEqual(h.calls.sessionMessages, [[NATIVE_SESSION, { dir: "/tmp/x" }]]);
  h.send({ id: 3, type: "send", sessionId: "s", message: FIRST_PROMPT });
  assert.equal((await h.next((f) => f.id === 3)).ok, true);
  assert.equal(seen.length, 1);
  assert.equal(seen[0].resume, NATIVE_SESSION);
  assert.equal(seen[0].resumeSessionAt, HEAD);
});

test("open with resumeAt is refused for an unknown uuid, a sub-agent message, a bad shape or a create", async () => {
  const history = [{ type: "assistant", uuid: HEAD }, { type: "assistant", uuid: "9e1d0f55-0000-4000-8000-000000000001", parent_tool_use_id: "toolu_1" }];
  for (const resumeAt of ["9e1d0f55-0000-4000-8000-0000000000ff", "9e1d0f55-0000-4000-8000-000000000001", "not-a-uuid", 7]) {
    const h = boot("multi_turn", { history });
    const res = await resumeAtOpen(h, { resumeAt });
    assert.equal(res.ok, false, String(resumeAt));
    assert.equal(res.error.cause.kind, "invalid_request");
  }
  const h = boot("multi_turn", { history });
  const res = await resumeAtOpen(h, { operation: "create", engineSessionId: undefined, resumeAt: HEAD });
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "invalid_request");
});

test("open with resumeAt needs getSessionMessages in the SDK", async () => {
  const h = boot("multi_turn", { withSessionMessages: false });
  const res = await resumeAtOpen(h, { resumeAt: HEAD });
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.name, "UnsupportedRequest");
});

test("open without resumeAt does not read the history", async () => {
  const h = boot("multi_turn");
  assert.equal((await resumeAtOpen(h, {})).ok, true);
  assert.deepEqual(h.calls.sessionMessages, []);
});

test("closeSession closes the live query once, and the session is unknown afterwards", async () => {
  const h = boot("multi_turn");
  await open(h);
  h.send({ id: 3, type: "send", sessionId: "s", message: "Respond with exactly: first fixture turn complete" });
  await h.next((f) => f.type === "runDone" && f.sendId === 3);
  h.send({ id: 4, type: "closeSession", sessionId: "s" });
  const closed = await h.next((f) => f.id === 4);
  assert.equal(closed.ok, true);
  assert.equal(h.calls.closes, 1);
  h.send({ id: 5, type: "send", sessionId: "s", message: "again" });
  const refused = await h.next((f) => f.id === 5);
  assert.equal(refused.ok, false);
  assert.equal(refused.error.cause.name, "UnknownSession");
  // Closing a session that is not open is a no-op, not an error.
  h.send({ id: 6, type: "closeSession", sessionId: "s" });
  assert.equal((await h.next((f) => f.id === 6)).ok, true);
  assert.equal(h.calls.closes, 1);
});

test("a steer or a cancel for a run that is not open is refused and touches nothing", async () => {
  const h = boot("multi_turn");
  await open(h);
  h.send({ id: 3, type: "steer", sendId: 99, messageId: "m1", text: "go left" });
  const steer = await h.next((f) => f.id === 3);
  assert.equal(steer.ok, true);
  assert.equal(steer.result.outcome, "rejected");
  assert.equal(steer.result.messageId, "m1");
  h.send({ id: 4, type: "cancel", sendId: 99 });
  const cancel = await h.next((f) => f.id === 4);
  assert.equal(cancel.result.cancelled, false);
  assert.equal(h.replay.calls.interrupts ?? 0, 0);
});

// The spawn hook of a confined session: the fake query only records its options, so a test drives
// `spawnClaudeCodeProcess` itself and reads the environment the child actually got. A confinement
// whose prefix runs node makes the child print its own environment, which no platform-specific shell
// is needed for.
const ENV_PRINTING_PREFIX = [process.execPath, "-e", "process.stdout.write(JSON.stringify(process.env))"];

async function confinedSpawn(confinement) {
  const seen = [];
  const query = ({ options }) => {
    seen.push(options);
    return { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true }) }), close() {} };
  };
  const h = boot("multi_turn", { query });
  await open(h, {}, { confinement });
  h.send({ id: 3, type: "send", sessionId: "s", message: FIRST_PROMPT });
  assert.equal((await h.next((f) => f.id === 3)).ok, true);
  assert.equal(seen.length, 1, "the session asked the SDK for one query");
  return { h, spawn: seen[0].spawnClaudeCodeProcess };
}

test("confinement: only the two Claude prefixes are added and the confinement env is not overridden", async () => {
  const confinement = {
    argvPrefix: ENV_PRINTING_PREFIX,
    env: { PATH: "/confinement-path", CLAUDE_CODE_KEPT: "from-confinement" },
    cwd: process.cwd(),
    writeRoots: [process.cwd()],
  };
  const { h, spawn } = await confinedSpawn(confinement);
  // The SDK hands back the host's own environment; only the Claude prefixes may join the allowlist.
  const child = spawn({
    command: "ignored-after-the-prefix",
    args: [],
    env: {
      CLAUDE_CODE_ALLOWED: "one",
      CLAUDE_AGENT_SDK_ALLOWED: "two",
      NOT_A_PREFIX: "three",
      PATH: "from-the-sdk",
      CLAUDE_CODE_KEPT: "from-the-sdk",
    },
  });
  // The keys are logged before the child is started: the frame comes from the same spawn hook.
  const log = await h.next((f) => f.type === "log" && f.event.type === "envAdded");
  assert.deepEqual(log.event.keys, ["CLAUDE_CODE_ALLOWED", "CLAUDE_AGENT_SDK_ALLOWED"]);
  const env = await new Promise((resolve, reject) => {
    let text = "";
    child.stdout.on("data", (chunk) => (text += chunk));
    child.on("error", reject);
    child.on("close", () => resolve(JSON.parse(text || "{}")));
  });
  assert.equal(env.CLAUDE_CODE_ALLOWED, "one");
  assert.equal(env.CLAUDE_AGENT_SDK_ALLOWED, "two");
  assert.equal(env.NOT_A_PREFIX, undefined, "only the two Claude prefixes are added");
  // A key the confinement allowlist already carries is never overridden by the SDK's copy.
  assert.equal(env.PATH, "/confinement-path");
  assert.equal(env.CLAUDE_CODE_KEPT, "from-confinement");
});

