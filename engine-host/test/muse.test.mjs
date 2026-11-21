import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { museFamily, museLaunch, SDK_VERSION, verbatimPath } from "../src/families/muse.mjs";
import { runHost } from "../src/host.mjs";
import { replayMuse } from "./replayMuse.mjs";

const fixture = (n) => fileURLToPath(new URL(`./fixtures/muse/${n}.ndjson`, import.meta.url));
const MODEL = "muse-spark-1.3-contributor";

function boot({ spawn, ...familyOptions }) {
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
  runHost({ family: museFamily({ loadSdk: async () => ({ spawnMspConnection: spawn }), ...familyOptions }), input, output });
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
  return { next, send, all, input };
}

const replayHost = (source, familyOptions = {}) => {
  const replay = replayMuse(source);
  return { replay, h: boot({ spawn: replay.spawn, ...familyOptions }) };
};
const settle = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const runs = (h, sendId) => h.all.filter((f) => f.type === "runDone" && f.sendId === sendId);

async function handshake(h) {
  await h.next((f) => f.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((f) => f.id === 1);
}

async function open(h, extra = {}, id = 2, sessionId = "s") {
  h.send({ id, type: "open", operation: "create", sessionId, cwd: "/work", options: { model: MODEL, ...extra.options }, ...extra.request });
  const res = await h.next((f) => f.id === id);
  assert.equal(res.ok, true, JSON.stringify(res));
  return res;
}

const ready = async (name, extra) => {
  const { replay, h } = replayHost(fixture(name));
  await handshake(h);
  const opened = await open(h, extra);
  return { replay, h, opened };
};

test("hello names the family, the pinned SDK version and the requests it implements", async () => {
  const { h } = replayHost(fixture("permission"));
  const hello = await h.next((f) => f.type === "hello");
  assert.equal(hello.family, "muse");
  assert.equal(hello.sdk.name, "@muse-code/sdk");
  assert.deepEqual(hello.capabilities, ["steer"]);
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.equal(pkg.dependencies["@muse-code/sdk"], SDK_VERSION);
  assert.equal(hello.sdk.version, SDK_VERSION);
});

test("permission: every ask is a frame, allow is once-only, deny rejects, one runDone per send", async () => {
  const { replay, h, opened } = await ready("permission");
  assert.equal(opened.result.engineSessionId, replay.sessionId());

  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  const accepted = await h.next((f) => f.id === 3);
  assert.equal(accepted.ok, true);
  assert.ok(accepted.result.turnId, "turn id reported");
  const ask1 = await h.next((f) => f.type === "permission" && f.sendId === 3);
  assert.equal(ask1.tool, "bash");
  assert.match(ask1.input.rawArgs, /touch muse-approved\.txt/);
  assert.equal(ask1.input.subject.command, "touch muse-approved.txt");
  // The engine's own choices travel as received, including the always-allow one the core never picks.
  assert.deepEqual(ask1.options.map((o) => o.choiceId), ["allow_once", "allow_local_prefix", "abort"]);
  h.send({ id: 4, type: "respond", requestId: ask1.requestId, decision: { behavior: "allow" } });
  assert.deepEqual((await h.next((f) => f.id === 4)).result, null);
  const done1 = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done1.ok, true);
  assert.equal(done1.result.reason, "completed");
  assert.equal(done1.result.threadDisposition, "reusable");

  h.send({ id: 5, type: "send", sessionId: "s", message: replay.promptTexts()[1] });
  assert.equal((await h.next((f) => f.id === 5)).ok, true);
  const ask2 = await h.next((f) => f.type === "permission" && f.sendId === 5);
  assert.notEqual(ask2.requestId, ask1.requestId);
  h.send({ id: 6, type: "respond", requestId: ask2.requestId, decision: { behavior: "deny", message: "no" } });
  await h.next((f) => f.id === 6);
  const done2 = await h.next((f) => f.type === "runDone" && f.sendId === 5);
  assert.equal(done2.result.reason, "completed");

  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.equal(runs(h, 5).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
  // approval/decide carried allow_once, then abort (Reject): never allow_local_prefix.
  const decides = replay.outbound.filter((f) => f.method === "approval/decide").map((f) => f.params.choiceId);
  assert.deepEqual(decides, ["allow_once", "abort"]);
});

test("a late respond is ignored and logged, never sent to the engine", async () => {
  const { replay, h } = await ready("permission");
  h.send({ id: 3, type: "respond", requestId: "p-99", decision: { behavior: "allow" } });
  assert.equal((await h.next((f) => f.id === 3)).ok, true);
  assert.ok(h.all.some((f) => f.type === "log" && f.event.type === "lateRespond"));
  assert.ok(!replay.outbound.some((f) => f.method === "approval/decide"));
});

test("subagent approval: the child's ask is a permission frame answered by respond", async () => {
  const { replay, h } = await ready("subagent_approval");
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  await h.next((f) => f.id === 3);
  const ask = await h.next((f) => f.type === "permission");
  assert.equal(ask.input.subagentOrigin.subagentId, "subagent-1");
  assert.equal(ask.sendId, 3);
  // Muse repeats the request as a server request and as notifications: one frame only.
  h.send({ id: 4, type: "respond", requestId: ask.requestId, decision: { behavior: "allow" } });
  await h.next((f) => f.id === 4);
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done.result.reason, "completed");
  await settle();
  assert.equal(h.all.filter((f) => f.type === "permission").length, 1);
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("subagent interrupt: cancel interrupts the turn and the run ends cancelled, once", async () => {
  const { replay, h } = await ready("subagent_interrupt", { options: { disableSandbox: true } });
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  await h.next((f) => f.id === 3);
  const delta = await h.next((f) => f.type === "delta" && f.update.params?.item?.kind === "subagent");
  assert.equal(delta.sendId, 3);
  h.send({ id: 4, type: "cancel", sendId: 3 });
  assert.deepEqual((await h.next((f) => f.id === 4)).result, { cancelled: true });
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done.ok, true);
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
  assert.ok(replay.outbound.some((f) => f.method === "turn/interrupt"));
});

test("workflow: the run is held while the workflow runs and ends on the follow-up turn", async () => {
  const { replay, h } = await ready("workflow", { options: { disableSandbox: true } });
  h.send({ id: 3, type: "send", sessionId: "s", message: replay.promptTexts()[0] });
  await h.next((f) => f.id === 3);
  const done = await h.next((f) => f.type === "runDone" && f.sendId === 3);
  assert.equal(done.result.reason, "completed");

  const started = h.all.find((f) => f.type === "background" && f.event.type === "started");
  const ended = h.all.find((f) => f.type === "background" && f.event.type === "ended");
  assert.equal(started.event.kind, "workflow");
  assert.equal(started.event.id, ended.event.id);
  assert.equal(ended.event.status, "completed");
  assert.equal(started.sessionId, "s");
  const at = (pred) => h.all.findIndex(pred);
  const startedAt = at((f) => f.type === "background" && f.event.type === "started");
  const endedAt = at((f) => f.type === "background" && f.event.type === "ended");
  const doneAt = at((f) => f.type === "runDone");
  const turnDoneAts = h.all.flatMap((f, i) => (f.type === "delta" && f.update.method === "turn/completed" ? [i] : []));
  assert.equal(turnDoneAts.length, 2);
  assert.ok(startedAt < turnDoneAts[0], "reported before the first turn ends");
  assert.ok(turnDoneAts[0] < endedAt && endedAt < doneAt, "first turn ended, workflow ended, then runDone");
  assert.ok(doneAt > turnDoneAts[1], "runDone follows the follow-up turn");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("confinement: argvPrefix prefixes the muse serve command and env is the whole environment", async () => {
  process.env.ORBYN_TEST_LEAK = "leak";
  process.env.META_API_KEY = "secret";
  const { replay, h } = replayHost(fixture("permission"));
  await handshake(h);
  const confinement = { argvPrefix: ["/usr/bin/sandbox-exec", "-f", "/p.sb", "--"], env: { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1" }, cwd: "/work", writeRoots: ["/work"] };
  await open(h, { options: { musePath: "/opt/muse" }, request: { confinement } });
  const [spawn] = replay.spawns;
  assert.equal(spawn.command, "/usr/bin/sandbox-exec");
  assert.deepEqual(spawn.args, ["-f", "/p.sb", "--", "/opt/muse", "serve", "--trust-workspace"]);
  assert.deepEqual(spawn.env, { PATH: "/bin", HOME: "/h", ORBYN_WORKER_ID: "w-1", MUSE_NO_AUTO_UPDATE: "1" });
  assert.equal(spawn.cwd, "/work");
  const added = h.all.find((f) => f.type === "log" && f.event.type === "envAdded");
  assert.deepEqual(added.event.keys, ["MUSE_NO_AUTO_UPDATE"]);
  assert.ok(!JSON.stringify(h.all).includes("secret"), "no credential in any frame");
  delete process.env.ORBYN_TEST_LEAK;
  delete process.env.META_API_KEY;
});

test("without confinement the host environment is used but META_API_KEY is dropped", async () => {
  process.env.META_API_KEY = "secret";
  const { replay, h } = replayHost(fixture("permission"));
  await handshake(h);
  await open(h);
  const [spawn] = replay.spawns;
  assert.equal(spawn.command, "muse");
  assert.equal(spawn.env.META_API_KEY, undefined);
  assert.equal(spawn.env.MUSE_NO_AUTO_UPDATE, "1");
  delete process.env.META_API_KEY;
});

test("open refusals start nothing: bad prefix, attach, fork, resume without an id", async () => {
  const { replay, h } = replayHost(fixture("permission"));
  await handshake(h);
  const refuse = async (id, request, options) => {
    h.send({ id, type: "open", operation: "create", sessionId: `s${id}`, cwd: "/work", options: options ?? {}, ...request });
    return h.next((f) => f.id === id);
  };
  const bad = await refuse(2, { confinement: { argvPrefix: ["ok", ""] } });
  assert.equal(bad.error.cause.kind, "invalid_request");
  const attach = await refuse(3, {}, { serverUrl: "http://127.0.0.1:1" });
  assert.equal(attach.error.cause.kind, "unsupported");
  const fork = await refuse(4, { operation: "fork", engineSessionId: "x" });
  assert.equal(fork.error.cause.kind, "unsupported");
  const resume = await refuse(5, { operation: "resume" });
  assert.equal(resume.error.cause.kind, "invalid_request");
  assert.equal(replay.spawns.length, 0);
});

test("a missing muse binary is a plain engine error", async () => {
  const spawn = () => ({
    child: { stderrTail: [] },
    exited: new Promise(() => {}),
    async initialize() {
      throw Object.assign(new Error("spawn muse ENOENT"), { name: "MuseHostStartError", failure: { kind: "spawnFailed", error: { code: "ENOENT" } }, stderrTail: [] });
    },
    async close() {},
  });
  const h = boot({ spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "create", sessionId: "s", cwd: "/work", options: {} });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "engine");
  assert.match(res.error.cause.message, /Muse Code is not installed/);
});

// Synthetic scenarios: the recorded session and turn framing from the permission transcript, then frames
// written here. They cover requests T3's fixtures do not hold, so they pin this adapter, not Muse.
const base = readFileSync(fixture("permission"), "utf8").trim().split("\n").map((l) => JSON.parse(l));
const IDS = base[0].metadata.commandIds.flat();
const SID = IDS[0];
const TURN = IDS[2];
const frameOf = (entry) => entry.frame;
const head = base.slice(0, 9); // header, host_start, initialize, initialized, session/start and its answers
const inbound = (label, frame) => ({ type: "emit_inbound", label, frame });
const outboundEntry = (label, frame) => ({ type: "expect_outbound", label, frame });
const note = (method, params) => inbound(method, { jsonrpc: "2.0", method, params: { sessionId: SID, ...params } });
const turnStart = (text = "hi") => [
  outboundEntry("turn/start", { jsonrpc: "2.0", id: 3, method: "turn/start", params: { sessionId: SID, input: [{ type: "text", text }], workspaceRoots: ["<workspace>"], ifBusy: "queue", commandId: TURN } }),
  inbound("response:turn/start", { jsonrpc: "2.0", id: 3, result: { commandId: TURN, status: "accepted", turnId: TURN, startedNewTurn: true, disposition: "started" } }),
  note("turn/started", { turnId: TURN, commandId: TURN }),
];
const completed = (terminal = "completed", extra = {}) => note("turn/completed", { turnId: TURN, terminal, ...extra });
const ask = (approvalId, choices, extra = {}) => ({ approvalId, turnId: TURN, toolName: "bash", rawArgs: "{}", subject: { kind: "shell", command: "x" }, currentRequirementId: { approvalId, sourceIndex: 0 }, availableChoices: choices, ...extra });

async function synthetic(body, options = {}) {
  const { replay, h } = replayHost([...head, ...body]);
  await handshake(h);
  await open(h, options);
  return { replay, h };
}

test("only always-allow offered: allow is answered with the reject choice, never the persistent one", async () => {
  const choices = [
    { choiceId: "allow_session", label: "Allow for the session", decision: "approvedForSession", scope: "session" },
    { choiceId: "allow_local_prefix", label: "Always", decision: "approvedPolicyAmendment", scope: "localPersistent" },
    { choiceId: "abort", label: "Reject", decision: "abort", scope: "once" },
  ];
  const body = [
    ...turnStart(),
    note("approval/requested", ask("a1", choices)),
    outboundEntry("approval/decide", { jsonrpc: "2.0", id: 4, method: "approval/decide", params: { sessionId: SID, approvalId: "a1", requirementId: { approvalId: "a1", sourceIndex: 0 }, choiceId: "abort", commandId: IDS[3] } }),
    inbound("response:approval/decide", { jsonrpc: "2.0", id: 4, result: { commandId: IDS[3], status: "accepted", approvalId: "a1", terminal: true } }),
    completed(),
  ];
  const { replay, h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  const frame = await h.next((f) => f.type === "permission");
  h.send({ id: 4, type: "respond", requestId: frame.requestId, decision: { behavior: "allow" } });
  await h.next((f) => f.id === 4);
  await h.next((f) => f.type === "runDone");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("a second stage of the same approval is a new permission frame", async () => {
  const once = [{ choiceId: "allow_once", label: "Allow once", decision: "approved", scope: "once" }, { choiceId: "abort", label: "Reject", decision: "abort", scope: "once" }];
  const decide = (id, cmd, index) => [
    outboundEntry("approval/decide", { jsonrpc: "2.0", id, method: "approval/decide", params: { sessionId: SID, approvalId: "a1", requirementId: { approvalId: "a1", sourceIndex: index }, choiceId: "allow_once", commandId: cmd } }),
    inbound("response:approval/decide", { jsonrpc: "2.0", id, result: { commandId: cmd, status: "accepted", approvalId: "a1", terminal: index === 1 } }),
  ];
  const stage = (index) => ask("a1", once, { currentRequirementId: { approvalId: "a1", sourceIndex: index } });
  const body = [...turnStart(), note("approval/requested", stage(0)), ...decide(4, IDS[3], 0), note("approval/updated", stage(0)), note("approval/updated", stage(1)), ...decide(5, IDS[4], 1), note("approval/resolved", { approvalId: "a1", decision: "approved" }), completed()];
  const { replay, h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  const first = await h.next((f) => f.type === "permission");
  h.send({ id: 4, type: "respond", requestId: first.requestId, decision: { behavior: "allow" } });
  await h.next((f) => f.id === 4);
  const second = await h.next((f) => f.type === "permission");
  assert.notEqual(second.requestId, first.requestId);
  assert.equal(second.input.currentRequirementId.sourceIndex, 1);
  h.send({ id: 5, type: "respond", requestId: second.requestId, decision: { behavior: "allow" } });
  await h.next((f) => f.id === 5);
  await h.next((f) => f.type === "runDone");
  assert.equal(h.all.filter((f) => f.type === "permission").length, 2);
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("steer sends turn/steer for the running turn and reports queued or rejected", async () => {
  const body = [
    ...turnStart(),
    outboundEntry("turn/steer", { jsonrpc: "2.0", id: 4, method: "turn/steer", params: { sessionId: SID, expectedTurnId: TURN, input: [{ type: "text", text: "also this" }], commandId: IDS[3] } }),
    inbound("response:turn/steer", { jsonrpc: "2.0", id: 4, result: { commandId: IDS[3], status: "accepted", turnId: TURN } }),
    outboundEntry("turn/steer", { jsonrpc: "2.0", id: 5, method: "turn/steer", params: { sessionId: SID, expectedTurnId: TURN, input: [{ type: "text", text: "late" }], commandId: IDS[4] } }),
    inbound("response:turn/steer", { jsonrpc: "2.0", id: 5, error: { code: -32010, message: "turn is not running", data: { kind: "notFound" } } }),
  ];
  const { replay, h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  h.send({ id: 4, type: "steer", sendId: 3, messageId: "m1", text: "also this" });
  assert.deepEqual((await h.next((f) => f.id === 4)).result, { outcome: "accepted", messageId: "m1" });
  h.send({ id: 5, type: "steer", sendId: 3, messageId: "m2", text: "late" });
  assert.deepEqual((await h.next((f) => f.id === 5)).result, { outcome: "rejected", messageId: "m2" });
  assert.deepEqual(replay.failures, []);
  h.send({ id: 6, type: "steer", sendId: 99, messageId: "m3", text: "x" });
  assert.equal((await h.next((f) => f.id === 6)).result.outcome, "rejected");
});

test("a question is a frame and its answers go to userInput/answer", async () => {
  const body = [
    ...turnStart(),
    note("userInput/requested", { userInputId: "u1", turnId: TURN, questions: [{ id: "q1", header: "Pick", question: "Which color?", options: [{ label: "Red" }, { label: "Blue" }], selection: { mode: "single" } }] }),
    outboundEntry("userInput/answer", { jsonrpc: "2.0", id: 4, method: "userInput/answer", params: { sessionId: SID, userInputId: "u1", answers: [{ questionId: "q1", selectedLabel: "Blue" }], commandId: IDS[3] } }),
    inbound("response:userInput/answer", { jsonrpc: "2.0", id: 4, result: { commandId: IDS[3], status: "accepted" } }),
    note("userInput/settled", { userInputId: "u1" }),
    completed(),
  ];
  const { replay, h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  const q = await h.next((f) => f.type === "question");
  assert.equal(q.sendId, 3);
  assert.equal(q.input.questions[0].question, "Which color?");
  h.send({ id: 4, type: "respond", requestId: q.requestId, decision: { answers: { "Which color?": "Blue" } } });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  assert.equal((await h.next((f) => f.type === "runDone")).result.reason, "completed");
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("a failed turn maps its quota to rate_limit with a reset time and a plain sentence", async () => {
  const body = [...turnStart(), completed("failed", { error: { message: "Usage limit reached", quota: { resetsAtMs: 1_800_000_000_000 } } })];
  const { h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.ok, false);
  assert.equal(done.result.reason, "failed");
  assert.equal(done.error.cause.kind, "rate_limit");
  assert.deepEqual(done.result.retry, { why: "rate_limit", resetsAt: 1_800_000_000 });
  assert.equal(done.error.method, "send");
});

test("usage from the turn counters is reported on completion", async () => {
  const usage = { inputTokens: 10, outputTokens: 5, cachedTokens: 2, reasoningTokens: 1, cacheReadTokens: 2 };
  const body = [...turnStart(), completed("completed", { usage })];
  const { h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  const done = await h.next((f) => f.type === "runDone");
  assert.deepEqual(done.result.usage, { inputTokens: 10, outputTokens: 5, cachedInputTokens: 2, reasoningTokens: 1 });
});

test("the engine process exiting mid-run fails the run once and marks the thread broken", async () => {
  const { replay, h } = await synthetic([...turnStart()]);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  replay.exit();
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.ok, false);
  assert.equal(done.result.threadDisposition, "broken");
  assert.equal(done.error.cause.kind, "engine");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  h.send({ id: 4, type: "send", sessionId: "s", message: "again" });
  const refused = await h.next((f) => f.id === 4);
  assert.equal(refused.error.cause.kind, "session_lost");
});

test("closeSession mid-run interrupts the turn and ends the run cancelled, once", async () => {
  const body = [
    ...turnStart(),
    outboundEntry("turn/interrupt", { jsonrpc: "2.0", id: 4, method: "turn/interrupt", params: { sessionId: SID, turnId: TURN, commandId: IDS[3] } }),
    inbound("response:turn/interrupt", { jsonrpc: "2.0", id: 4, result: { commandId: IDS[3], status: "accepted", turnId: TURN } }),
  ];
  const { replay, h } = await synthetic(body);
  h.send({ id: 3, type: "send", sessionId: "s", message: "hi" });
  await h.next((f) => f.id === 3);
  h.send({ id: 4, type: "closeSession", sessionId: "s" });
  assert.equal((await h.next((f) => f.id === 4)).ok, true);
  const done = await h.next((f) => f.type === "runDone");
  assert.equal(done.result.reason, "cancelled");
  await settle();
  assert.equal(runs(h, 3).length, 1);
  assert.deepEqual(replay.failures, []);
  assert.ok(replay.closed(), "the muse serve process was shut down");
});

test("resume asks Muse for the stored session and sets the approval mode", async () => {
  const resume = [
    outboundEntry("session/resume", { jsonrpc: "2.0", id: 2, method: "session/resume", params: { sessionId: "old-session", excludeItems: true, permissionProfile: ":ask-me", commandId: IDS[0] } }),
    inbound("response:session/resume", { jsonrpc: "2.0", id: 2, result: { session: { sessionId: "old-session" } } }),
    outboundEntry("session/setApprovalMode", { jsonrpc: "2.0", id: 3, method: "session/setApprovalMode", params: { sessionId: "old-session", mode: "promptUnmatched", commandId: IDS[1] } }),
    inbound("response:session/setApprovalMode", { jsonrpc: "2.0", id: 3, result: { commandId: IDS[1], status: "accepted" } }),
  ];
  const replay = replayMuse([...base.slice(0, 5), ...resume]);
  const h = boot({ spawn: replay.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "resume", engineSessionId: "old-session", sessionId: "s", cwd: "/work", options: {} });
  const res = await h.next((f) => f.id === 2);
  assert.deepEqual(res.result, { engineSessionId: "old-session" });
  assert.deepEqual(replay.failures, []);
  assert.deepEqual(replay.remaining(), []);
});

test("resume of a session Muse no longer has is session_lost", async () => {
  const resume = [
    outboundEntry("session/resume", { jsonrpc: "2.0", id: 2, method: "session/resume", params: { sessionId: "gone", excludeItems: true, permissionProfile: ":ask-me", commandId: IDS[0] } }),
    inbound("response:session/resume", { jsonrpc: "2.0", id: 2, error: { code: -32004, message: "session not found", data: { kind: "sessionNotFound" } } }),
  ];
  const replay = replayMuse([...base.slice(0, 5), ...resume]);
  const h = boot({ spawn: replay.spawn });
  await handshake(h);
  h.send({ id: 2, type: "open", operation: "resume", engineSessionId: "gone", sessionId: "s", cwd: "/work", options: {} });
  const res = await h.next((f) => f.id === 2);
  assert.equal(res.ok, false);
  assert.equal(res.error.cause.kind, "session_lost");
  assert.ok(replay.closed(), "the failed open stopped its process");
});

test("close stops every muse serve process", async () => {
  const { replay, h } = await synthetic([]);
  h.send({ id: 5, type: "close" });
  await h.next((f) => f.id === 5);
  await settle();
  assert.ok(replay.closed());
});

test("requests the family does not implement are UnsupportedRequest", async () => {
  const { h } = await synthetic([]);
  for (const [i, type] of ["fork", "rollback", "models", "usage", "stopTask"].entries()) {
    h.send({ id: 10 + i, type, sessionId: "s" });
    const res = await h.next((f) => f.id === 10 + i);
    assert.equal(res.error.cause.name, "UnsupportedRequest", type);
  }
});

void frameOf;

test("Windows launcher: PATH and PATHEXT resolve muse, a .cmd runs under cmd.exe, other platforms are unchanged", () => {
  const env = { Path: "C:\\bin;C:\\npm", PathExt: ".EXE;.CMD", comspec: "C:\\Windows\\System32\\cmd.exe" };
  const only = (hit) => ({ platform: "win32", exists: (p) => p === hit });
  assert.deepEqual(museLaunch("muse", env, only("C:\\npm\\muse.CMD")), { command: "C:\\Windows\\System32\\cmd.exe", args: ["/d", "/c", "C:\\npm\\muse.CMD"] });
  assert.deepEqual(museLaunch("muse", env, only("C:\\bin\\muse.EXE")), { command: "C:\\bin\\muse.EXE", args: [] });
  assert.deepEqual(museLaunch("C:\\Program Files\\m\\muse.cmd", env, only("C:\\Program Files\\m\\muse.cmd")).args, ["/d", "/c", "@", "C:\\Program Files\\m\\muse.cmd"]);
  assert.deepEqual(museLaunch("muse", env, only("nothing")), { command: "muse", args: [] }, "unresolved: let the spawn report it");
  assert.deepEqual(museLaunch("muse", env, { platform: "darwin" }), { command: "muse", args: [] });
});

test("verbatim paths for Windows turn roots", () => {
  assert.equal(verbatimPath("C:\\work\\p"), "\\\\?\\C:\\work\\p");
  assert.equal(verbatimPath("\\\\srv\\share\\p"), "\\\\?\\UNC\\srv\\share\\p");
  assert.equal(verbatimPath("\\\\?\\C:\\p"), "\\\\?\\C:\\p");
});
