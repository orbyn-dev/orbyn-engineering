import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";

// OpenCode on the host: the promise client of @opencode/client (MIT, bundled) over `opencode serve`
// of the person's own `opencode` binary (never bundled). One server per distinct confinement, shared
// by that confinement's sessions. Policy stays in Rust: sessions are created asking for everything and
// every ask becomes a `permission` frame. Ideas for server ownership and the event mapping follow T3
// Code's provider-opencode (MIT, Copyright (c) 2026 T3 Tools Inc.); see test/fixtures/NOTICE.
// Nothing here has run against a real model turn yet; see the README.

export const SDK_VERSION = "2.3.2"; // keep equal to package.json (a test checks it)

const ASK_EVERYTHING = [{ action: "*", resource: "*", effect: "ask" }];
const START_TIMEOUT_MS = 20_000;
const CONNECT_TIMEOUT_MS = 10_000;
const INFO_TIMEOUT_MS = 5_000;
const EXECUTION_END = new Set(["session.execution.succeeded", "session.execution.failed", "session.execution.interrupted"]);

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

export function opencodeFamily({ loadSdk, launch = spawnServer }) {
  return {
    name: "opencode",
    sdk: { name: "@opencode/client", version: SDK_VERSION },
    capabilities: ["steer", "fork"],
    create(ctx) {
      const sessions = new Map(); // host sessionId -> session
      const servers = new Map(); // confinement key -> Promise<server>
      const pendingPermissions = new Map(); // requestId -> {sessionId, engineRequestId}
      let permissionCounter = 0;
      let sdkPromise;
      const sdk = () => (sdkPromise ??= loadSdk());

      // Frames for a run wait until the send response is written, so a fast engine never lets a
      // delta or runDone overtake the response that announces the run.
      const emit = (run, frame) => (run.open ? ctx.send(frame) : run.buffer.push(frame));

      const finish = (session, frame) => {
        const run = session.run;
        if (!run || run.done) return;
        run.done = true;
        session.run = null;
        for (const [id, p] of pendingPermissions) if (p.sessionId === session.id) pendingPermissions.delete(id);
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

      const onEvent = (server, event) => {
        const session = server.sessions.get(event?.data?.sessionID);
        if (!session) return;
        const run = session.run;
        const data = event.data;
        if (event.type === "permission.asked") {
          const requestId = `p-${++permissionCounter}`;
          pendingPermissions.set(requestId, { sessionId: session.id, engineSessionId: session.engineSessionId, engineRequestId: data.id });
          const { id: _id, sessionID: _sid, ...input } = data;
          const frame = { type: "permission", ...(run ? { sendId: run.sendId } : {}), requestId, tool: data.action, input };
          if (run) emit(run, frame);
          else ctx.send(frame);
          return;
        }
        if (event.type === "session.usage.updated") session.usage = { ...data.tokens, costUsd: data.cost };
        if (run) emit(run, { type: "delta", sendId: run.sendId, update: event });
        if (!EXECUTION_END.has(event.type) || !run) return;
        if (event.type === "session.execution.succeeded") {
          finish(session, { ok: true, result: { reason: "completed", threadDisposition: "reusable", ...(session.usage ? { usage: session.usage } : {}) } });
        } else if (event.type === "session.execution.interrupted") {
          finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
        } else {
          const error = data.error ?? {};
          const text = String(error.message ?? "OpenCode ended the run with an error.");
          const kind = kindOfStatus(error.status, text);
          finish(session, failFrame(Object.assign(new Error(text), { name: "OpenCodeExecutionError", code: error.type, ...(typeof error.status === "number" ? { status: error.status } : {}) }), kind, { retry: retryFor(kind, error.status) }));
        }
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
        const server = { key, handle, sessions: new Map(), abort: new AbortController(), closing: false, stopped: false };
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
              }
              const session = { id: sessionId, cwd, options, server, engineSessionId, run: null };
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
              const pending = pendingPermissions.get(request.requestId);
              if (!pending) {
                ctx.log({ type: "lateRespond", requestId: request.requestId });
                return null;
              }
              pendingPermissions.delete(request.requestId);
              const session = sessions.get(pending.sessionId);
              if (!session?.server) return null;
              const d = request.decision ?? {};
              // Once-only: the reply is never "always", so the server never saves a rule.
              const reply = d.behavior === "allow" ? { decision: "once" } : { decision: "reject", message: d.message ?? "Denied." };
              try {
                await session.server.client.permission.reply({ sessionID: pending.engineSessionId, requestID: pending.engineRequestId, ...reply });
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
              if (server && server.sessions.size === 0) {
                servers.delete(server.key);
                stopServer(server);
              }
              return null;
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
