// Cursor on the host: the public surface of @cursor/sdk (Agent.create / Agent.resume, agent.send with
// onDelta, run.wait / steer / cancel, agent.close). The shape follows the T3 Code Cursor provider
// (MIT, packages/provider-cursor, commit 9168ca7); see test/fixtures/NOTICE. The SDK is not bundled and
// the family starts only behind ORBYN_ENGINE_HOST_CURSOR=1 (see main.mjs and the README).
//
// Pooled: one host process serves every Cursor session, so a per-run argv prefix cannot be applied.
// `open` with confinement.argvPrefix fails with kind "unsupported"; the core starts the whole host
// under the prefix instead. The Cursor SDK has no interactive approval request, so this family never
// sends a `permission` or `question` frame. Nothing here has run against the real SDK.

const CLOSE_GRACE_MS = 3000;
const MAX_CAUSE_DEPTH = 8;
const NO_PROGRESS = "Agent turn stopped after repeated resume attempts made no progress";
const NETWORK_CODES = new Set(["ECONNRESET", "ETIMEDOUT", "ECONNREFUSED", "ENOTFOUND", "EAI_AGAIN", "EPIPE"]);

function get(target, key) {
  try {
    return target?.[key];
  } catch {
    return undefined;
  }
}

function chain(cause) {
  const out = [];
  const seen = new Set();
  let current = cause;
  while (typeof current === "object" && current !== null && !seen.has(current) && out.length <= MAX_CAUSE_DEPTH) {
    seen.add(current);
    out.push(current);
    current = get(current, "cause");
  }
  return out;
}

/** Maps an SDK error to the closed `kind` set of the protocol. Names are the SDK's public error classes. */
export function errorKind(cause) {
  const links = chain(cause);
  if (links.some((c) => get(c, "name") === "AbortError")) return "cancelled";
  for (const c of links) {
    const name = String(get(c, "name") ?? "");
    const message = String(get(c, "message") ?? "");
    const status = get(c, "status");
    const code = get(c, "code");
    if (name === "AuthenticationError" || status === 401 || status === 403 || /api key|unauthenticated|unauthorized|not logged in|authentication/i.test(message)) return "auth";
    if (name === "RateLimitError" || status === 429 || /rate.?limit|usage limit|too many requests/i.test(message)) return "rate_limit";
    if (name === "ConfigurationError") return "invalid_request";
    if (NETWORK_CODES.has(code) || /ConnectError: \[(unavailable|deadline_exceeded|aborted)\]|RetriableError/.test(message)) return "network";
  }
  return "engine";
}

/** SDK promises that nothing awaits reject after a run or agent closes; they must not stop the host. */
export function isBenignSdkRejection(reason) {
  if (!(reason instanceof Error)) return false;
  if (reason.name === "WriteIterableClosedError") return true;
  if (reason.name === "ConnectError" && get(reason.cause, "code") === "ERR_STREAM_WRITE_AFTER_END") return true;
  if (reason.name === "AbortError" && typeof reason.stack === "string" && /^\s+at (?:\S+\.)?onStall\b/m.test(reason.stack)) return true;
  const syscall = get(reason, "syscall");
  const args = get(reason, "spawnargs");
  return typeof syscall === "string" && syscall.startsWith("spawn") && Array.isArray(args) &&
    args.some((a) => typeof a === "string" && /dump_zsh_state|dump_bash_state|__CURSOR_SANDBOX_ENV_RESTORE/.test(a));
}

const USAGE_KEYS = ["inputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens", "reasoningTokens"];

/** Adds one turn-ended usage to the run's total (T3 sums them); undefined when nothing was reported. */
function addUsage(total, usage) {
  if (typeof usage !== "object" || usage === null) return total;
  const out = { ...(total ?? {}) };
  for (const key of USAGE_KEYS) if (typeof usage[key] === "number") out[key] = (out[key] ?? 0) + usage[key];
  out.totalTokens = ["inputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens"].reduce((n, k) => n + (out[k] ?? 0), 0);
  return out;
}

function protocolError(name, message, kind) {
  return Object.assign(new Error(message), { name, kind });
}

export function cursorFamily({ loadSdk, env = process.env }) {
  return {
    name: "cursor",
    sdk: { name: "@cursor/sdk", version: env.ORBYN_CURSOR_SDK_VERSION ?? "unknown" },
    capabilities: ["steer"],
    create(ctx) {
      const sessions = new Map(); // sessionId -> {id, agent, cwd, run}
      const runs = new Map(); // sendId -> session
      const starting = new Set();
      const pendingCancel = new Set();
      const apiKey = typeof env.CURSOR_API_KEY === "string" && env.CURSOR_API_KEY !== "" ? env.CURSOR_API_KEY : undefined;
      let sdkPromise;
      const sdk = () => (sdkPromise ??= loadSdk());

      const redact = (text) => (apiKey && apiKey.length >= 8 ? String(text).split(apiKey).join("[redacted]") : String(text));
      // A copy of the error chain with the API key removed: nothing from the SDK crosses the wire raw.
      const sanitize = (cause, depth = 0) => {
        if (typeof cause !== "object" || cause === null) return Object.assign(new Error(redact(cause)), { name: "Error" });
        const inner = depth < MAX_CAUSE_DEPTH ? get(cause, "cause") : undefined;
        const out = new Error(redact(get(cause, "message") ?? String(cause)), inner === undefined ? undefined : { cause: sanitize(inner, depth + 1) });
        out.name = typeof get(cause, "name") === "string" && get(cause, "name") !== "" ? get(cause, "name") : "Error";
        const stack = get(cause, "stack");
        out.stack = typeof stack === "string" ? redact(stack) : undefined;
        if (typeof get(cause, "code") === "string") out.code = get(cause, "code");
        if (typeof get(cause, "status") === "number") out.status = get(cause, "status");
        out.kind = typeof get(cause, "kind") === "string" ? get(cause, "kind") : errorKind(cause);
        return out;
      };
      const fail = (cause) => {
        throw sanitize(cause);
      };

      const finish = (session, run, frame) => {
        if (run.done) return;
        run.done = true;
        if (session.run === run) session.run = null;
        runs.delete(run.sendId);
        ctx.send({ type: "runDone", sendId: run.sendId, ...frame });
      };

      const cancelRun = async (run) => {
        try {
          await run.handle?.cancel?.();
        } catch (cause) {
          if (errorKind(cause) !== "cancelled") throw cause;
        }
      };

      const settle = (session, run, result) => {
        const status = result?.status;
        // The SDK's own result usage wins; without it (a cancel, the no-progress error) the turn-ended totals stand in.
        result = { ...result, usage: result?.usage ?? run.usage };
        if (status === "finished") {
          return finish(session, run, { ok: true, result: { reason: "completed", threadDisposition: "reusable", usage: result.usage } });
        }
        if (status === "cancelled") {
          return finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable", usage: result.usage } });
        }
        const text = String(get(result?.error, "message") ?? "");
        // Cursor ends a run with this error after the answer was already delivered (T3 adapter rule).
        if (status === "error" && run.turnEnded && text === NO_PROGRESS) {
          return finish(session, run, { ok: true, result: { reason: "completed", threadDisposition: "reusable", usage: result.usage } });
        }
        return failRun(session, run, result?.error ?? new Error(`Cursor ended the run with status ${status ?? "unknown"}.`));
      };

      const failRun = (session, run, cause) => {
        const clean = sanitize(cause);
        const kind = clean.kind;
        if (kind === "cancelled") {
          return finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable", ...(run.usage ? { usage: run.usage } : {}) } });
        }
        const retry = kind === "rate_limit" || kind === "network" ? { retry: { why: kind } } : {};
        finish(session, run, {
          ok: false,
          error: { method: "send", cause: { name: clean.name, message: clean.message, kind, ...(clean.code ? { code: clean.code } : {}), ...(clean.status ? { status: clean.status } : {}) } },
          result: { reason: "failed", threadDisposition: kind === "session_lost" ? "broken" : "reusable", ...(run.usage ? { usage: run.usage } : {}), ...retry },
        });
      };

      const closeAgent = async (agent) => {
        let timer;
        try {
          await Promise.race([
            Promise.resolve(agent?.close?.()),
            new Promise((resolve) => (timer = setTimeout(resolve, CLOSE_GRACE_MS))),
          ]);
        } catch {
          // closing is best effort: the session is gone either way
        } finally {
          clearTimeout(timer);
        }
      };

      return {
        async handle(request) {
          switch (request.type) {
            case "open": {
              const { sessionId, cwd, options = {}, confinement } = request;
              if (confinement?.argvPrefix !== undefined) {
                throw protocolError("UnsupportedConfinement", "Cursor runs in one shared host and cannot take a per-run argv prefix; start the host under the prefix instead.", "unsupported");
              }
              if (request.operation === "fork") throw protocolError("UnsupportedRequest", "Cursor sessions cannot be forked.", "unsupported");
              if (!["create", "resume"].includes(request.operation)) throw protocolError("InvalidRequest", `unknown open operation ${request.operation}`, "invalid_request");
              if (options.apiKey !== undefined || options.local?.apiKey !== undefined) {
                throw protocolError("InvalidRequest", "The API key is set with CURSOR_API_KEY when the host starts, never in a request.", "invalid_request");
              }
              if (sessions.has(sessionId)) throw protocolError("InvalidRequest", "this session is already open", "invalid_request");
              if (confinement?.env && Object.keys(confinement.env).length > 0) {
                ctx.log({ type: "confinementEnvNotApplied", keys: Object.keys(confinement.env), reason: "pooled host" });
              }
              if (request.operation === "resume" && !request.engineSessionId) throw protocolError("InvalidRequest", "resume needs engineSessionId", "invalid_request");
              const { Agent } = await sdk().catch(fail);
              const agentOptions = {
                ...options,
                ...(apiKey ? { apiKey } : {}),
                local: { enableAgentRetries: true, sandboxOptions: { enabled: true }, ...options.local, ...(cwd ? { cwd } : {}) },
              };
              let agent;
              try {
                agent = request.operation === "resume" ? await Agent.resume(request.engineSessionId, agentOptions) : await Agent.create(agentOptions);
              } catch (cause) {
                const clean = sanitize(cause);
                if (request.operation === "resume" && clean.kind === "engine" && /not found|no such agent|unknown agent|does not exist/i.test(clean.message)) clean.kind = "session_lost";
                throw clean;
              }
              sessions.set(sessionId, { id: sessionId, agent, run: null });
              return { engineSessionId: agent.agentId };
            }
            case "send": {
              const session = sessions.get(request.sessionId);
              if (!session) throw protocolError("UnknownSession", "unknown session", "invalid_request");
              if (session.run) throw protocolError("RunInProgress", "a run is already open on this session", "invalid_request");
              const message = request.message;
              const text = typeof message === "string" ? message : message?.text;
              if (typeof message === "object" && message !== null && (message.attachments?.length ?? 0) > 0) {
                throw protocolError("UnsupportedRequest", "Cursor attachments are not supported yet.", "unsupported");
              }
              if (typeof text !== "string" || text.trim() === "") throw protocolError("InvalidRequest", "send needs message text", "invalid_request");
              const { apiKey: _ignored, onDelta: _ignoredDelta, ...sendOptions } = request.options ?? {};
              const sendId = request.id;
              const run = { sendId, done: false, ready: false, pending: [], handle: null, turnEnded: false, usage: undefined };
              const emit = (update) => ctx.send({ type: "delta", sendId, update });
              session.run = run;
              runs.set(sendId, session);
              starting.add(sendId);
              try {
                run.handle = await session.agent.send(text, {
                  ...sendOptions,
                  onDelta: async ({ update }) => {
                    if (run.done) return;
                    if (update?.type === "turn-ended") {
                      run.turnEnded = true;
                      run.usage = addUsage(run.usage, update.usage);
                    }
                    if (run.ready) emit(update);
                    else run.pending.push(update);
                  },
                });
              } catch (cause) {
                if (session.run === run) session.run = null;
                runs.delete(sendId);
                pendingCancel.delete(sendId);
                return fail(cause);
              } finally {
                starting.delete(sendId);
              }
              if (pendingCancel.delete(sendId) || run.done) {
                await cancelRun(run).catch(() => {});
                if (session.run === run) session.run = null;
                runs.delete(sendId);
                throw protocolError("AbortError", "The run was cancelled while starting.", "cancelled");
              }
              // The response goes out before any delta or runDone for this sendId.
              const ready = new Promise((resolve) =>
                setImmediate(() => {
                  run.ready = true;
                  for (const update of run.pending.splice(0)) emit(update);
                  resolve();
                }),
              );
              Promise.resolve(run.handle.wait()).then(
                (result) => ready.then(() => settle(session, run, result)),
                (cause) => ready.then(() => failRun(session, run, cause)),
              );
              return { runId: run.handle.id ?? run.handle.runId, engineSessionId: session.agent.agentId };
            }
            case "steer": {
              const session = runs.get(request.sendId);
              const run = session?.run;
              if (!run?.handle?.steer) return { outcome: "rejected", messageId: request.messageId };
              if (typeof request.text !== "string" || request.text.trim() === "") throw protocolError("InvalidRequest", "steer needs text", "invalid_request");
              const outcome = await run.handle.steer(request.text).catch(fail);
              // Anything but a delivered steer (the SDK's "revert_to_followup") is declined; the core sends a follow-up.
              return { outcome: outcome === "complete_delivered" ? "accepted" : "rejected", messageId: request.messageId };
            }
            case "cancel": {
              const session = runs.get(request.sendId);
              const run = session?.run;
              if (!run) {
                if (starting.has(request.sendId)) {
                  pendingCancel.add(request.sendId);
                  return { cancelled: true };
                }
                return { cancelled: false };
              }
              if (run.handle) await cancelRun(run).catch(fail);
              else pendingCancel.add(request.sendId);
              return { cancelled: true };
            }
            case "respond": {
              // The Cursor SDK raises no approval or question, so there is never a pending request to answer.
              ctx.log({ type: "lateRespond", requestId: request.requestId });
              return null;
            }
            case "closeSession": {
              const session = sessions.get(request.sessionId);
              if (!session) return null;
              sessions.delete(request.sessionId);
              const run = session.run;
              if (run && !run.handle) {
                run.done = true; // still starting: the send fails as cancelled, no runDone
              } else if (run) {
                await cancelRun(run).catch(() => {});
                finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              }
              await closeAgent(session.agent);
              return null;
            }
            default:
              throw protocolError("UnsupportedRequest", `unsupported request ${request.type}`, "unsupported");
          }
        },
        async close() {
          const all = [...sessions.values()];
          sessions.clear();
          await Promise.all(all.map(async (s) => {
            if (s.run) await cancelRun(s.run).catch(() => {});
            s.run = null;
            await closeAgent(s.agent);
          }));
        },
      };
    },
  };
}
