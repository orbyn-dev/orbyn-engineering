import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";

// OpenCode on the host: the promise client of @opencode/client (MIT, bundled) over `opencode serve`
// of the person's own `opencode` binary (never bundled). One server per distinct confinement, shared
// by that confinement's sessions. Policy stays in Rust: sessions are created asking for everything and
// every ask becomes a `permission` frame. Ideas for server ownership and the event mapping follow T3
// Code's provider-opencode (MIT, Copyright (c) 2026 T3 Tools Inc.); see test/fixtures/NOTICE.
// Beyond turns: models, usage, history, stopTask, background, `background` frames (shells and subagent
// sessions) that hold a run per docs/engine-run-contract.md, and `question` frames from OpenCode forms.
// The README says what ran against a real `opencode` and what did not.

export const SDK_VERSION = "2.3.2"; // keep equal to package.json (a test checks it)

const ASK_EVERYTHING = [{ action: "*", resource: "*", effect: "ask" }];
const START_TIMEOUT_MS = 20_000;
const CONNECT_TIMEOUT_MS = 10_000;
const INFO_TIMEOUT_MS = 5_000;
const EXECUTION_END = new Set(["session.execution.succeeded", "session.execution.failed", "session.execution.interrupted"]);
const WAKE_GRACE_MS = 3000; // after the last subagent ends, wait this long for the parent's own wake turn
const MAX_HOLD_MS = 30 * 60_000; // a held run is released after this long whatever the subagents do
const MODELS_TIMEOUT_MS = 10_000;
const HISTORY_PAGE = 100;
const HISTORY_MAX_PAGES = 500;
const HISTORY_BUDGET_BYTES = 6 * 1024 * 1024; // under the 8 MiB frame cap, leaving room for the envelope

const hostError = (name, message, kind, cause) => Object.assign(new Error(message, cause ? { cause } : undefined), { name, kind });

function withTimeout(promise, ms, make) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => (timer = setTimeout(() => reject(make()), ms)))]).finally(() => clearTimeout(timer));
}

function kindOfStatus(status, text = "") {
  if (status === 401 || status === 403 || /unauthori[sz]ed|authentication|api key|not logged in/i.test(text)) return "auth";
  if (status === 429 || /rate.?limit|usage limit|quota/i.test(text)) return "rate_limit";
  if (status === 503 || status === 529) return "network";
  return "engine";
}

/** Maps a thrown client error to the protocol's closed `kind` set. */
function clientError(cause, fallback = "engine") {
  if (cause?.kind) return cause;
  const name = cause?.name;
  let kind = fallback;
  if (name === "SessionNotFoundError" || cause?.status === 404) kind = "session_lost";
  else if (name === "UnauthorizedError") kind = "auth";
  else if (name === "ClientError" && cause.reason === "Transport") kind = "network";
  else if (name === "ClientError" || typeof cause?.status === "number") kind = kindOfStatus(cause.status, String(cause?.message));
  return Object.assign(new Error(String(cause?.message ?? cause), { cause }), { name: name ?? "Error", kind, ...(typeof cause?.status === "number" ? { status: cause.status } : {}) });
}

const retryFor = (kind, status) =>
  kind === "rate_limit" ? { why: "rate_limit" } : kind === "network" ? { why: status === 529 ? "overloaded" : "network" } : undefined;

/** Starts `opencode serve` and resolves with its URL once it prints where it listens. */
export async function spawnServer({ command, args, env, cwd }) {
  const child = spawn(command, args, { cwd, env, stdio: ["ignore", "pipe", "pipe"] });
  let exitCode;
  let onExit;
  const exited = new Promise((resolve) => (onExit = resolve));
  child.on("exit", (code, signal) => {
    exitCode = code ?? signal;
    onExit(exitCode);
  });
  child.stderr.pipe(process.stderr, { end: false });
  const url = await withTimeout(
    new Promise((resolve, reject) => {
      let out = "";
      child.on("error", reject);
      child.on("exit", () => reject(new Error("OpenCode stopped before it was ready.")));
      child.stdout.on("data", (chunk) => {
        out += chunk;
        for (const line of out.split("\n").slice(0, -1)) {
          const match = line.match(/server listening on\s+(https?:\/\/\S+)/i);
          if (match) resolve(match[1]);
        }
        out = out.slice(-4096);
      });
    }),
    START_TIMEOUT_MS,
    () => new Error("Timed out waiting for OpenCode to start."),
  ).catch((cause) => {
    child.kill("SIGTERM");
    throw cause;
  });
  child.stdout.resume();
  return {
    url,
    exited,
    stop() {
      if (exitCode !== undefined) return;
      child.kill("SIGTERM");
      setTimeout(() => exitCode === undefined && child.kill("SIGKILL"), 3000).unref();
    },
  };
}

const tokensOf = (t = {}) => ({ input: t.input ?? 0, output: t.output ?? 0, reasoning: t.reasoning ?? 0, cache: { read: t.cache?.read ?? 0, write: t.cache?.write ?? 0 } });
// T3 Code's context figure: what the last step put in the window, output included.
const contextUsed = (t) => t.input + t.cache.read + t.cache.write + t.output;

/** What a model picker needs. Settings, headers, body and package are never forwarded: they can hold provider keys. */
const modelEntry = (m, isDefault) => ({
  id: m.id,
  providerID: m.providerID,
  name: m.name,
  efforts: (m.variants ?? []).map((v) => v.id),
  capabilities: m.capabilities,
  limit: m.limit,
  status: m.status,
  enabled: m.enabled,
  ...(m.family ? { family: m.family } : {}),
  default: isDefault,
});

/** Typed answers for one form field from the text the person gave. Undefined means "not a valid value". */
function fieldAnswer(field, raw) {
  const text = String(raw);
  switch (field.type) {
    case "multiselect":
      return text.split(",").map((v) => v.trim()).filter(Boolean);
    case "number": {
      const n = Number(text);
      return text.trim() !== "" && Number.isFinite(n) ? n : undefined;
    }
    case "integer": {
      const n = Number(text);
      return text.trim() !== "" && Number.isInteger(n) ? n : undefined;
    }
    case "boolean":
      return /^(true|yes)$/i.test(text.trim()) ? true : /^(false|no)$/i.test(text.trim()) ? false : undefined;
    default:
      return text;
  }
}

export function opencodeFamily({ loadSdk, launch = spawnServer, wakeGraceMs = WAKE_GRACE_MS, maxHoldMs = MAX_HOLD_MS }) {
  return {
    name: "opencode",
    sdk: { name: "@opencode/client", version: SDK_VERSION },
    capabilities: ["steer", "fork", "models", "usage", "readHistory", "stopTask", "background"],
    create(ctx) {
      const sessions = new Map(); // host sessionId -> session
      const servers = new Map(); // confinement key -> Promise<server>
      const pending = new Map(); // requestId -> {kind: "permission" | "question", sessionId, engineSessionId, ...}
      let permissionCounter = 0;
      let questionCounter = 0;
      let sdkPromise;
      const sdk = () => (sdkPromise ??= loadSdk());

      // Frames for a run wait until the send response is written, so a fast engine never lets a
      // delta or runDone overtake the response that announces the run.
      const emit = (run, frame) => (run.open ? ctx.send(frame) : run.buffer.push(frame));

      const finish = (session, frame) => {
        const run = session.run;
        if (!run || run.done) return;
        run.done = true;
        clearTimeout(run.grace);
        clearTimeout(run.limit);
        session.run = null;
        for (const [id, p] of pending) if (p.sessionId === session.id) pending.delete(id);
        emit(run, { type: "runDone", sendId: run.sendId, ...frame });
        run.ended?.();
      };

      const failFrame = (cause, kind, extra = {}) => ({
        ok: false,
        result: { reason: "failed", threadDisposition: "reusable", ...extra },
        error: { method: "send", cause: { name: cause.name ?? "Error", message: cause.message ?? String(cause), kind, ...(cause.code ? { code: cause.code } : {}), ...(cause.status ? { status: cause.status } : {}) } },
      });

      const lose = (server, cause) => {
        if (server.closing) return;
        server.closing = true;
        servers.delete(server.key);
        server.tree.clear();
        server.shells.clear();
        for (const session of server.sessions.values()) {
          session.server = null;
          finish(session, failFrame(hostError("OpenCodeServerLost", "OpenCode stopped. Your work in the project is kept; send your message again.", "engine", cause), "engine", { threadDisposition: "broken" }));
        }
        stopServer(server);
      };

      const stopServer = (server) => {
        server.closing = true;
        server.abort.abort();
        if (!server.stopped) {
          server.stopped = true;
          server.handle.stop();
        }
      };

      // ---- the session tree: subagent sessions and shells under a root session ----

      const emitBackground = (session, event) => {
        const frame = { type: "background", sessionId: session.id, event };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };
      const rootOf = (server, id) => (id === undefined ? undefined : server.sessions.get(id) ?? server.tree.get(id)?.root);
      const anyRunning = (session) => [...(session.server?.tree.values() ?? [])].some((n) => n.root === session && n.running);

      // Task ids are announced to the core with the tool call that started them, which OpenCode reports in a
      // separate `session.tool.progress` event, so a task is announced once both are known.
      const announce = (task) => {
        if (task.announced) return;
        task.announced = true;
        task.root.tasks.set(task.id, { kind: task.kind, cwd: task.cwd });
        emitBackground(task.root, task.startFrame());
      };
      const link = (server, taskId, toolCallId) => {
        const task = server.shells.get(taskId) ?? server.tree.get(taskId);
        if (task) {
          task.toolCallId = toolCallId;
          announce(task);
        } else server.links.set(taskId, toolCallId);
      };
      const registerTask = (server, task) => {
        const toolCallId = server.links.get(task.id);
        server.links.delete(task.id);
        if (toolCallId !== undefined) task.toolCallId = toolCallId;
        return task;
      };
      const endTask = (task, extra) => {
        if (task.ended) return;
        task.ended = true;
        announce(task);
        emitBackground(task.root, { type: "ended", id: task.id, kind: task.kind, ...extra });
      };
      const shellStarted = (shell) => ({ type: "started", id: shell.id, kind: "shell", ...(shell.toolCallId ? { toolCallId: shell.toolCallId } : {}), command: shell.command });
      const agentStarted = (node, server) => ({
        type: "started",
        id: node.id,
        kind: "agent",
        ...(node.toolCallId ? { toolCallId: node.toolCallId } : {}),
        ...(node.title ? { description: node.title } : {}),
        background: server.toolInputs.get(node.toolCallId)?.background === true,
      });

      const armGrace = (session, run) => {
        clearTimeout(run.grace);
        run.grace = setTimeout(() => {
          if (session.run === run && !run.waking && !anyRunning(session)) finish(session, terminalFrame(session, run.heldEvent));
        }, wakeGraceMs);
        run.grace.unref?.();
      };
      // A subagent started or ended: a held run waits for the last one, then for the parent's wake turn.
      const treeChanged = (session) => {
        const run = session.run;
        if (!run?.held) return;
        if (anyRunning(session)) return void clearTimeout(run.grace);
        if (!run.waking) armGrace(session, run);
      };

      const terminalFrame = (session, event) => {
        const data = event.data;
        if (event.type === "session.execution.succeeded") {
          return { ok: true, result: { reason: "completed", threadDisposition: "reusable", ...(session.usage ? { usage: session.usage } : {}) } };
        }
        if (event.type === "session.execution.interrupted") return { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } };
        const error = data.error ?? {};
        const text = String(error.message ?? "OpenCode ended the run with an error.");
        const kind = kindOfStatus(error.status, text);
        return failFrame(Object.assign(new Error(text), { name: "OpenCodeExecutionError", code: error.type, ...(typeof error.status === "number" ? { status: error.status } : {}) }), kind, { retry: retryFor(kind, error.status) });
      };

      const onRootEnd = (session, event) => {
        const run = session.run;
        if (!run) return;
        if (run.held) {
          run.waking = false;
          run.heldEvent = event;
          if (anyRunning(session)) return;
          return finish(session, terminalFrame(session, event));
        }
        // Run-end contract, rule 1: a subagent the engine started keeps the run open. Only a successful
        // result is held; a failure or a cancel ends the run at once.
        if (event.type === "session.execution.succeeded" && anyRunning(session)) {
          run.held = true;
          run.heldEvent = event;
          ctx.log({ type: "runHeld", why: "subagent" });
          run.limit = setTimeout(() => {
            if (session.run !== run) return;
            ctx.log({ type: "holdExpired" });
            finish(session, terminalFrame(session, run.heldEvent));
          }, maxHoldMs);
          run.limit.unref?.();
          return;
        }
        finish(session, terminalFrame(session, event));
      };

      // ---- questions: an OpenCode form raised by the question tool ----

      const onForm = (server, form) => {
        const session = rootOf(server, form.sessionID);
        if (!session) return;
        const kind = form.metadata?.kind;
        if (kind !== "question") {
          // A host that cannot ask refuses (protocol, ground rule 1): never leave a form open.
          ctx.log({ type: "unsupportedForm", kind: kind ?? null });
          void server.client.session.form.cancel({ sessionID: form.sessionID, formID: form.id, message: "The host cannot ask this." }).catch(() => {});
          return;
        }
        const toolInput = server.toolInputs.get(form.metadata?.tool?.id);
        const fields = form.fields;
        const questions = Array.isArray(toolInput?.questions) ? toolInput.questions : undefined;
        const texts = fields.map((f, i) => questions?.[i]?.question ?? f.description ?? f.title ?? f.key);
        let input = toolInput;
        if (!questions) {
          ctx.log({ type: "questionInputDerived" });
          input = {
            questions: fields.map((f, i) => ({
              question: texts[i],
              header: f.title ?? f.key,
              options: (f.options ?? []).map((o) => ({ label: o.label ?? o.value, description: o.description ?? "" })),
              ...(f.type === "multiselect" ? { multiple: true } : {}),
            })),
          };
        }
        const requestId = `q-${++questionCounter}`;
        pending.set(requestId, { kind: "question", sessionId: session.id, engineSessionId: form.sessionID, formId: form.id, fields, texts });
        const run = session.run;
        const frame = { type: "question", ...(run ? { sendId: run.sendId } : {}), requestId, tool: "question", input };
        if (run) emit(run, frame);
        else ctx.send(frame);
      };

      const onEvent = (server, event) => {
        const data = event?.data;
        const type = event?.type;
        if (type === "form.created") return onForm(server, data.form);
        if (type === "form.replied" || type === "form.cancelled") {
          for (const [id, p] of pending) if (p.kind === "question" && p.formId === data.id) pending.delete(id);
          return;
        }
        if (type === "shell.created") {
          const info = data.info;
          const root = rootOf(server, info?.metadata?.sessionID);
          if (!root) return;
          const shell = registerTask(server, { id: info.id, kind: "shell", root, command: info.command, cwd: info.cwd, announced: false, ended: false });
          shell.startFrame = () => shellStarted(shell);
          server.shells.set(shell.id, shell);
          if (shell.toolCallId !== undefined) announce(shell);
          if (root.run) emit(root.run, { type: "delta", sendId: root.run.sendId, update: event });
          return;
        }
        if (type === "shell.exited" || type === "shell.deleted") {
          const shell = server.shells.get(data.id);
          if (!shell) return;
          if (shell.root.run) emit(shell.root.run, { type: "delta", sendId: shell.root.run.sendId, update: event });
          // The first end wins: an interrupt deletes the shell before it reports its exit.
          if (type === "shell.deleted") endTask(shell, { status: "killed" });
          else endTask(shell, { status: data.status, ...(data.exit !== undefined ? { exit: data.exit } : {}) });
          return;
        }
        if (type === "session.created" && data.parentID && !server.tree.has(data.sessionID)) {
          const root = rootOf(server, data.parentID);
          if (root) {
            const node = registerTask(server, { id: data.sessionID, kind: "agent", root, parentId: data.parentID, title: data.title, running: true, announced: false, ended: false });
            node.startFrame = () => agentStarted(node, server);
            server.tree.set(node.id, node);
            if (node.toolCallId !== undefined) announce(node);
            treeChanged(root);
          }
        }
        const root = server.sessions.get(data?.sessionID);
        const node = root ? undefined : server.tree.get(data?.sessionID);
        const session = root ?? node?.root;
        if (!session) return;
        const run = session.run;
        if (type === "permission.asked") {
          const requestId = `p-${++permissionCounter}`;
          pending.set(requestId, { kind: "permission", sessionId: session.id, engineSessionId: data.sessionID, engineRequestId: data.id });
          const { id: _id, sessionID: _sid, ...input } = data;
          const frame = { type: "permission", ...(run ? { sendId: run.sendId } : {}), requestId, tool: data.action, input };
          if (run) emit(run, frame);
          else ctx.send(frame);
          return;
        }
        if (type === "session.tool.called") server.toolInputs.set(data.id, data.input);
        else if (type === "session.tool.success" || type === "session.tool.failed") server.toolInputs.delete(data.id);
        else if (type === "session.tool.progress") {
          const meta = data.metadata ?? {};
          if (typeof meta.shellID === "string") link(server, meta.shellID, data.id);
          if (typeof meta.sessionID === "string") link(server, meta.sessionID, data.id);
        }
        if (root) {
          if (type === "session.usage.updated") session.usage = { ...data.tokens, costUsd: data.cost };
          if ((type === "session.step.ended" || type === "session.step.failed") && data.tokens) session.lastStep = tokensOf(data.tokens);
          if (type === "session.execution.started" && run?.held) {
            run.waking = true; // the parent's own wake turn for a subagent's report
            clearTimeout(run.grace);
          }
        }
        if (run) emit(run, { type: "delta", sendId: run.sendId, update: event });
        if (node && type === "session.execution.started") {
          node.running = true;
          treeChanged(session);
        } else if (node && EXECUTION_END.has(type)) {
          node.running = false;
          endTask(node, { status: type === "session.execution.succeeded" ? "completed" : type === "session.execution.failed" ? "failed" : "cancelled" });
          treeChanged(session);
        }
        if (root && EXECUTION_END.has(type)) onRootEnd(session, event);
      };

      const startServer = async (key, confinement, options, cwd) => {
        const prefix = confinement?.argvPrefix;
        if (prefix !== undefined && !(Array.isArray(prefix) && prefix.every((p) => typeof p === "string" && p.length > 0))) {
          throw hostError("InvalidConfinement", "confinement.argvPrefix must be a list of non-empty strings.", "invalid_request");
        }
        const password = randomBytes(32).toString("base64url");
        const base = confinement?.env ?? process.env;
        const env = {};
        for (const [k, v] of Object.entries(base)) if (typeof v === "string" && k !== "OPENCODE_SERVER_PASSWORD" && k !== "OPENCODE_PASSWORD") env[k] = v;
        env.OPENCODE_PASSWORD = password;
        ctx.log({ type: "envAdded", keys: ["OPENCODE_PASSWORD"] });
        const [command, ...args] = [...(prefix ?? []), options.opencodePath ?? "opencode", "serve", "--hostname=127.0.0.1", "--port=0"];
        let handle;
        try {
          handle = await launch({ command, args, env, cwd: confinement?.cwd ?? cwd });
        } catch (cause) {
          if (cause?.code === "ENOENT") throw hostError("OpenCodeNotFound", "OpenCode is not installed or not on PATH.", "engine", cause);
          throw clientError(cause);
        }
        const server = { key, handle, sessions: new Map(), tree: new Map(), shells: new Map(), links: new Map(), toolInputs: new Map(), busy: 0, abort: new AbortController(), closing: false, stopped: false };
        try {
          const { OpenCode } = await sdk();
          server.client = OpenCode.make({ baseUrl: handle.url, headers: { Authorization: `Basic ${Buffer.from(`opencode:${password}`).toString("base64")}` } });
          // Subscribe before any work: the stream is volatile, and server.connected proves it is live.
          let connected;
          const ready = new Promise((resolve) => (connected = resolve));
          const stream = server.client.event.subscribe({}, { signal: server.abort.signal });
          void (async () => {
            try {
              for await (const event of stream) {
                if (event?.type === "server.connected") connected();
                onEvent(server, event);
              }
              lose(server, new Error("The OpenCode event stream ended."));
            } catch (cause) {
              lose(server, cause);
            }
          })();
          void Promise.resolve(handle.exited).then(() => lose(server, new Error("The OpenCode process exited.")));
          await withTimeout(ready, CONNECT_TIMEOUT_MS, () => hostError("OpenCodeNotReady", "Timed out waiting for the OpenCode event stream.", "network"));
          await withTimeout(server.client.server.info(), INFO_TIMEOUT_MS, () => hostError("OpenCodeNotReady", "Timed out waiting for the OpenCode server.", "network"));
        } catch (cause) {
          stopServer(server);
          throw clientError(cause);
        }
        return server;
      };

      const serverFor = (confinement, options, cwd) => {
        if (options.serverUrl !== undefined) {
          throw hostError("UnsupportedServer", "Attaching to a running OpenCode server is not supported.", "unsupported");
        }
        const key = JSON.stringify([confinement?.argvPrefix, confinement?.env, confinement?.cwd, options.opencodePath]);
        let promise = servers.get(key);
        if (!promise) {
          promise = startServer(key, confinement, options, cwd);
          servers.set(key, promise);
          promise.catch(() => servers.delete(key));
        }
        return promise;
      };

      const forkEngineSession = async (server, engineSessionId, before) => {
        const info = await server.client.session.fork({ sessionID: engineSessionId, ...(before ? { before } : {}) });
        // A fork inherits nothing the core decided: ask for everything again.
        await server.client.session.update({ sessionID: info.id, permissions: ASK_EVERYTHING });
        return info.id;
      };

      const sessionOf = (id) => {
        const session = sessions.get(id);
        if (!session) throw hostError("UnknownSession", "unknown session", "invalid_request");
        return session;
      };
      const runOf = (sendId) => [...sessions.values()].find((s) => s.run?.sendId === sendId);

      return {
        async handle(request) {
          switch (request.type) {
            case "open": {
              const { sessionId, cwd, options = {}, confinement } = request;
              const server = await serverFor(confinement, options, cwd);
              server.busy++;
              let engineSessionId;
              try {
                if (request.operation === "resume") {
                  engineSessionId = request.engineSessionId;
                  await server.client.session.get({ sessionID: engineSessionId });
                } else if (request.operation === "fork") {
                  engineSessionId = await forkEngineSession(server, request.engineSessionId, options.before);
                } else {
                  const info = await server.client.session.create({
                    location: { directory: cwd },
                    permissions: options.permissions ?? ASK_EVERYTHING,
                    ...(options.model ? { model: options.model } : {}),
                    ...(options.agent ? { agent: options.agent } : {}),
                    ...(options.title ? { title: options.title } : {}),
                  });
                  engineSessionId = info.id;
                }
              } catch (cause) {
                throw clientError(cause);
              } finally {
                server.busy--;
              }
              const session = { id: sessionId, cwd, options, server, engineSessionId, run: null, tasks: new Map(), lastStep: undefined };
              sessions.set(sessionId, session);
              server.sessions.set(engineSessionId, session);
              return { engineSessionId };
            }
            case "send": {
              const session = sessionOf(request.sessionId);
              if (session.run) throw hostError("RunInProgress", "a run is already open on this session", "invalid_request");
              if (!session.server) throw hostError("OpenCodeServerLost", "OpenCode stopped. Open the session again.", "session_lost");
              const text = typeof request.message === "string" ? request.message : request.message?.text;
              if (typeof text !== "string") throw hostError("InvalidMessage", "message must be text.", "invalid_request");
              const run = { sendId: request.id, open: false, buffer: [], done: false };
              session.run = run;
              let accepted;
              try {
                accepted = await session.server.client.session.prompt({ sessionID: session.engineSessionId, text });
              } catch (cause) {
                if (session.run === run) session.run = null;
                throw clientError(cause);
              }
              run.turnId = accepted?.id;
              setImmediate(() => {
                run.open = true;
                for (const frame of run.buffer.splice(0)) ctx.send(frame);
              });
              return { runId: `run-${request.id}`, engineSessionId: session.engineSessionId, ...(run.turnId ? { turnId: run.turnId } : {}) };
            }
            case "steer": {
              const session = runOf(request.sendId);
              if (!session?.server) return { outcome: "rejected", messageId: request.messageId };
              try {
                await session.server.client.session.prompt({ sessionID: session.engineSessionId, text: request.text, delivery: "steer" });
                return { outcome: "queued", messageId: request.messageId };
              } catch (cause) {
                ctx.log({ type: "steerRejected", kind: clientError(cause).kind });
                return { outcome: "rejected", messageId: request.messageId };
              }
            }
            case "cancel": {
              const session = runOf(request.sendId);
              if (!session?.server) return { cancelled: false };
              const run = session.run;
              if (run.held) {
                // Idle parent, subagents still working: stop them all, then end the run now.
                const ids = [session.engineSessionId, ...[...session.server.tree.values()].filter((n) => n.root === session && n.running).map((n) => n.id)];
                await Promise.allSettled(ids.map((sessionID) => session.server.client.session.interrupt({ sessionID })));
                finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
                return { cancelled: true };
              }
              const ended = new Promise((resolve) => (run.ended = resolve));
              try {
                await session.server.client.session.interrupt({ sessionID: session.engineSessionId });
              } catch (cause) {
                throw clientError(cause);
              }
              await Promise.race([ended, new Promise((r) => setTimeout(r, session.options.cancelGraceMs ?? 5000))]);
              finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              return { cancelled: true };
            }
            case "respond": {
              const entry = pending.get(request.requestId);
              if (!entry) {
                ctx.log({ type: "lateRespond", requestId: request.requestId });
                return null;
              }
              const session = sessions.get(entry.sessionId);
              if (!session?.server) {
                pending.delete(request.requestId);
                return null;
              }
              const d = request.decision ?? {};
              const { client } = session.server;
              if (entry.kind === "question") {
                if (d.behavior === "deny") {
                  pending.delete(request.requestId);
                  try {
                    await client.session.form.cancel({ sessionID: entry.engineSessionId, formID: entry.formId });
                  } catch (cause) {
                    throw clientError(cause);
                  }
                  return null;
                }
                if (typeof d.answers !== "object" || d.answers === null) throw hostError("InvalidAnswer", "Answer the question, or skip it.", "invalid_request");
                const answer = {};
                entry.fields.forEach((field, i) => {
                  if (field.hidden || field.type === "external") return;
                  const raw = d.answers[entry.texts[i]];
                  if (raw === undefined || (typeof raw === "string" && raw.trim() === "")) {
                    if (field.required === true) throw hostError("InvalidAnswer", `Answer the question: ${entry.texts[i]}`, "invalid_request");
                    return;
                  }
                  const value = fieldAnswer(field, raw);
                  if (value === undefined) throw hostError("InvalidAnswer", `That is not a valid answer to: ${entry.texts[i]}`, "invalid_request");
                  answer[field.key] = value;
                });
                if (Object.keys(answer).length === 0) throw hostError("InvalidAnswer", "Answer the question, or skip it.", "invalid_request");
                pending.delete(request.requestId);
                try {
                  await client.session.form.reply({ sessionID: entry.engineSessionId, formID: entry.formId, answer });
                } catch (cause) {
                  throw clientError(cause);
                }
                return null;
              }
              pending.delete(request.requestId);
              // Once-only: the reply is never "always", so the server never saves a rule.
              const reply = d.behavior === "allow" ? { decision: "once" } : { decision: "reject", message: d.message ?? "Denied." };
              try {
                await client.permission.reply({ sessionID: entry.engineSessionId, requestID: entry.engineRequestId, ...reply });
              } catch (cause) {
                throw clientError(cause);
              }
              return null;
            }
            case "fork": {
              const session = sessionOf(request.sessionId);
              if (!session.server) throw hostError("OpenCodeServerLost", "OpenCode stopped. Open the session again.", "session_lost");
              try {
                return { engineSessionId: await forkEngineSession(session.server, session.engineSessionId, request.atTurn) };
              } catch (cause) {
                throw clientError(cause);
              }
            }
            case "closeSession": {
              const session = sessions.get(request.sessionId);
              if (!session) return null;
              sessions.delete(request.sessionId);
              const server = session.server;
              if (session.run && server) {
                await server.client.session.interrupt({ sessionID: session.engineSessionId }).catch(() => {});
                finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              }
              server?.sessions.delete(session.engineSessionId);
              for (const map of [server?.tree, server?.shells]) for (const [id, task] of map ?? []) if (task.root === session) map.delete(id);
              for (const [id, p] of pending) if (p.sessionId === session.id) pending.delete(id);
              if (server && server.sessions.size === 0 && server.busy === 0) {
                servers.delete(server.key);
                stopServer(server);
              }
              return null;
            }
            case "models": {
              const options = { ...(request.opencodePath !== undefined ? { opencodePath: request.opencodePath } : {}) };
              const own = request.confinement !== undefined || request.opencodePath !== undefined;
              let server;
              let temporary = false;
              if (own) {
                const key = JSON.stringify([request.confinement?.argvPrefix, request.confinement?.env, request.confinement?.cwd, request.opencodePath]);
                temporary = !servers.has(key);
                server = await serverFor(request.confinement, options, request.cwd);
              } else {
                const live = [...servers.values()][0];
                temporary = live === undefined;
                server = await (live ?? serverFor(undefined, options, request.cwd));
              }
              server.busy++;
              try {
                const input = request.cwd ? { location: { directory: request.cwd } } : {};
                const [list, fallback] = await Promise.all([
                  withTimeout(server.client.model.list(input), MODELS_TIMEOUT_MS, () => hostError("OpenCodeTimeout", "Timed out reading OpenCode's models.", "network")),
                  // The default is a nicety: its failure never hides the list.
                  withTimeout(Promise.resolve().then(() => server.client.model.default(input)), MODELS_TIMEOUT_MS, () => new Error("timeout")).catch(() => null),
                ]);
                const chosen = fallback?.data;
                return list.data.map((m) => modelEntry(m, chosen != null && chosen.id === m.id && chosen.providerID === m.providerID));
              } catch (cause) {
                throw clientError(cause);
              } finally {
                server.busy--;
                if (temporary && server.sessions.size === 0 && server.busy === 0) {
                  servers.delete(server.key);
                  stopServer(server);
                }
              }
            }
            case "usage": {
              const session = sessionOf(request.sessionId);
              if (!session.server) throw hostError("OpenCodeServerLost", "OpenCode stopped. Open the session again.", "session_lost");
              const { client } = session.server;
              try {
                const info = await client.session.get({ sessionID: session.engineSessionId });
                let lastStep = session.lastStep;
                if (!lastStep) {
                  // A resumed session has seen no step yet: the newest assistant message carries the last one.
                  const page = await client.message.list({ sessionID: session.engineSessionId, order: "desc", limit: 1, type: "assistant" }).catch(() => null);
                  if (page?.data?.[0]?.tokens) lastStep = tokensOf(page.data[0].tokens);
                }
                let maxTokens = null;
                if (info.model) {
                  const list = await withTimeout(client.model.list({ location: { directory: session.cwd } }), MODELS_TIMEOUT_MS, () => new Error("timeout")).catch(() => null);
                  const m = list?.data?.find((x) => x.id === info.model.id && x.providerID === info.model.providerID);
                  if (m) maxTokens = m.limit?.input ?? m.limit?.context ?? null;
                }
                return {
                  ...tokensOf(info.tokens),
                  costUsd: info.cost ?? 0,
                  ...(info.model ? { model: { ...info.model } } : {}),
                  context: lastStep ? { usedTokens: contextUsed(lastStep), maxTokens } : null,
                };
              } catch (cause) {
                throw clientError(cause);
              }
            }
            case "readHistory": {
              const session = sessionOf(request.sessionId);
              if (!session.server) throw hostError("OpenCodeServerLost", "OpenCode stopped. Open the session again.", "session_lost");
              const messages = [];
              let bytes = 0;
              let input = { sessionID: session.engineSessionId, order: "asc", limit: HISTORY_PAGE };
              try {
                for (let page = 0; page < HISTORY_MAX_PAGES; page++) {
                  const res = await session.server.client.message.list(input);
                  for (const message of res.data) {
                    bytes += JSON.stringify(message).length;
                    if (bytes > HISTORY_BUDGET_BYTES) throw hostError("HistoryTooLarge", "This conversation is too large to read in one piece.", "host");
                    messages.push(message);
                  }
                  if (res.data.length === 0 || res.cursor?.next == null) break;
                  // Cursor pages must omit `order` or OpenCode rejects them (T3 Code's note).
                  input = { sessionID: session.engineSessionId, limit: HISTORY_PAGE, cursor: res.cursor.next };
                }
              } catch (cause) {
                throw clientError(cause);
              }
              return messages;
            }
            case "stopTask": {
              const session = sessionOf(request.sessionId);
              const task = session.tasks.get(request.taskId);
              if (!task || !session.server) return { stopped: false };
              try {
                if (task.kind === "shell") {
                  await session.server.client.shell.remove({ id: request.taskId, location: { directory: task.cwd ?? session.cwd } });
                  return { stopped: true };
                }
                const res = await session.server.client.session.interrupt({ sessionID: request.taskId });
                return { stopped: res?.interrupted === true };
              } catch (cause) {
                throw clientError(cause);
              }
            }
            case "background": {
              const session = runOf(request.sendId);
              if (!session?.server) return { backgrounded: false };
              try {
                await session.server.client.session.background({ sessionID: session.engineSessionId });
              } catch (cause) {
                throw clientError(cause);
              }
              return { backgrounded: true };
            }
            default:
              throw hostError("UnsupportedRequest", `unsupported request ${request.type}`, "unsupported");
          }
        },
        async close() {
          for (const promise of servers.values()) {
            const server = await promise.catch(() => null);
            if (server) stopServer(server);
          }
        },
      };
    },
  };
}
