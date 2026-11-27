import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { AsyncQueue } from "../asyncQueue.mjs";

// Claude on the host: query() from @anthropic-ai/claude-agent-sdk over the person's own `claude`.
// Policy stays in Rust: every tool request becomes a `permission` frame. The SDK is not bundled;
// see the README (Anthropic terms gate). Nothing here has run against the real SDK yet.

const EXTRA_ENV_PREFIXES = ["CLAUDE_CODE_", "CLAUDE_AGENT_SDK_"];
// The core refuses a credential-named variable from a confined child's allowlist (crates/orbyn-core/src/agents/driver/host/confinement.rs,
// `names_a_secret`). The SDK hands back the host process environment, so a key under an allowed prefix gets the same refusal here: a
// `CLAUDE_CODE_OAUTH_TOKEN` set in the service environment must not re-enter a confined worker. The core rule's account names cannot
// start with either prefix, so these credential parts are the whole rule that can be reached.
const SECRET_NAME_PARTS = ["SECRET", "TOKEN", "PASSWORD", "PASSPHRASE", "CREDENTIAL", "API_KEY", "ACCESS_KEY", "PRIVATE_KEY"];
const namesASecret = (name) => {
  const upper = name.toUpperCase();
  return SECRET_NAME_PARTS.some((part) => upper.includes(part)) || upper.endsWith("_KEY");
};
const TERMINAL_TASK_STATUSES = new Set(["completed", "failed", "killed", "stopped", "cancelled"]);
const TURN_HEADS_KEPT = 500;
const HISTORY_BUDGET_BYTES = 6 * 1024 * 1024; // the same limit as the OpenCode family's history
const SUBAGENT_ID = /^[A-Za-z0-9_-]{1,128}$/;
const SUBAGENT_LIMIT_MAX = 5000;

const hostError = (name, message, kind) => Object.assign(new Error(message), { name, ...(kind ? { kind } : {}) });
const historyTooLarge = () => hostError("HistoryTooLarge", "This conversation is too large to read in one piece.", "host");
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const invalidRequest = (message) => hostError("InvalidRequest", message, "invalid_request");
const unsupportedSdkMethod = (method) => Object.assign(new Error(`${method} is not available in this SDK`), { name: "UnsupportedRequest" });

/** The engine's own messages, unchanged, unless their JSON would not fit one frame. */
function boundedHistory(messages) {
  if (JSON.stringify(messages).length > HISTORY_BUDGET_BYTES) throw historyTooLarge();
  return messages;
}

function errorKind(status, text = "") {
  if (status === 401 || status === 403 || /not logged in|authenticat|oauth|\/login/i.test(text)) return "auth";
  if (status === 429) return "rate_limit";
  if (status === 529 || status === 503) return "network";
  if (/No conversation found/i.test(text)) return "session_lost";
  return "engine";
}

export function claudeFamily({ loadSdk }) {
  let probed;
  return {
    name: "claude",
    sdk: { name: "@anthropic-ai/claude-agent-sdk", version: process.env.ORBYN_CLAUDE_SDK_VERSION ?? "unknown" },
    capabilities: ["steer", "fork", "models", "usage", "readHistory", "readSubagent", "rollback", "stopTask", "background"],
    // Whether this build carries the SDK, asked once at init. Node caches the import, so a first run does not load it twice.
    probeSdk: () => (probed ??= Promise.resolve().then(loadSdk).then(() => true, () => false)),
    create(ctx) {
      const sessions = new Map();
      const pendingPermissions = new Map(); // requestId -> {resolve, sessionId}
      let permissionCounter = 0;
      let sdkPromise;
      const sdk = () => (sdkPromise ??= loadSdk());

      const finish = (session, frame) => {
        const run = session.run;
        if (!run || run.done) return;
        run.done = true;
        session.run = null;
        for (const [id, p] of pendingPermissions) {
          if (p.sessionId === session.id) {
            pendingPermissions.delete(id);
            p.resolve({ behavior: "deny", message: "The run ended." });
          }
        }
        ctx.send({ type: "runDone", sendId: run.sendId, ...frame });
      };
      const failure = (cause, kind) => ({
        ok: false,
        error: { method: "send", cause: { name: cause.name ?? "Error", message: cause.message ?? String(cause), kind } },
      });

      const spawnProcess = (session) => (spawnOptions) => {
        const c = session.confinement;
        if (!c?.argvPrefix?.length) {
          return spawn(spawnOptions.command, spawnOptions.args, {
            cwd: spawnOptions.cwd, env: spawnOptions.env, signal: spawnOptions.signal, stdio: ["pipe", "pipe", "pipe"],
          });
        }
        const env = { ...c.env };
        const added = [];
        for (const [k, v] of Object.entries(spawnOptions.env ?? {})) {
          if (!(k in env) && EXTRA_ENV_PREFIXES.some((p) => k.startsWith(p)) && !namesASecret(k)) {
            env[k] = v;
            added.push(k);
          }
        }
        ctx.log({ type: "envAdded", keys: added });
        const [cmd, ...prefixArgs] = c.argvPrefix;
        return spawn(cmd, [...prefixArgs, spawnOptions.command, ...spawnOptions.args], {
          cwd: c.cwd ?? spawnOptions.cwd, env, signal: spawnOptions.signal, stdio: ["pipe", "pipe", "pipe"],
        });
      };

      const canUseTool = (session) => (toolName, input, opts = {}) =>
        new Promise((resolve) => {
          const requestId = `p-${++permissionCounter}`;
          let settled = false;
          const settle = (v) => {
            if (settled) return;
            settled = true;
            pendingPermissions.delete(requestId);
            resolve(v);
          };
          pendingPermissions.set(requestId, { resolve: settle, sessionId: session.id, input, tool: toolName });
          opts.signal?.addEventListener("abort", () => settle({ behavior: "deny", message: "The run was cancelled." }), { once: true });
          ctx.send({ type: toolName === "AskUserQuestion" ? "question" : "permission", sendId: session.run?.sendId, requestId, tool: toolName, input });
        });

      const onMessage = (session, msg) => {
        if (msg.session_id && session.engineSessionId !== msg.session_id) {
          session.engineSessionId = msg.session_id;
          ctx.send({ type: "session", sendId: session.run?.sendId, engineSessionId: msg.session_id, turnId: session.run?.turnId });
        }
        const run = session.run;
        if (run) {
          ctx.send({ type: "delta", sendId: run.sendId, update: msg });
          // The point after this turn: its last top-level assistant message, not one from a subagent.
          if (msg.type === "assistant" && !msg.parent_tool_use_id && typeof msg.uuid === "string") {
            session.turnHeads.delete(run.turnId);
            session.turnHeads.set(run.turnId, msg.uuid);
            while (session.turnHeads.size > TURN_HEADS_KEPT) session.turnHeads.delete(session.turnHeads.keys().next().value);
          }
        }
        if (msg.type === "system" && msg.subtype === "task_started") {
          const holds = msg.task_type === "local_agent" || msg.task_type === "local_workflow";
          if (holds && run) run.held.add(msg.task_id);
          ctx.send({
            type: "background", sessionId: session.id,
            event: { type: "started", id: msg.task_id, kind: msg.task_type === "local_bash" ? "shell" : msg.task_type === "local_workflow" ? "workflow" : "agent", toolCallId: msg.tool_use_id },
          });
        } else if (msg.type === "system" && (msg.subtype === "task_notification" || msg.subtype === "task_updated")) {
          const status = msg.status ?? msg.patch?.status;
          if (msg.subtype === "task_notification" || TERMINAL_TASK_STATUSES.has(status)) {
            run?.held.delete(msg.task_id);
            ctx.send({ type: "background", sessionId: session.id, event: { type: "ended", id: msg.task_id, status } });
          }
        } else if (msg.type === "command_lifecycle" && run && msg.command_uuid === run.uuid) {
          run.lifecycle = true;
          if (msg.state === "started") run.started = true;
        } else if (msg.type === "result") {
          session.resultWaiter?.();
          if (!run) return;
          // rule 2: with lifecycle frames, a result before our prompt's command started belongs to a
          // leftover turn the engine began for its own notifications; it never ends this run.
          if (run.lifecycle && !run.started) {
            ctx.log({ type: "leftoverResult", sendId: run.sendId });
            return;
          }
          // rule 1: background work the engine started holds the first result; the follow-up turn ends the run.
          if (run.held.size > 0 && !run.holdUsed) {
            run.holdUsed = true;
            ctx.log({ type: "holdingResult", sendId: run.sendId, tasks: run.held.size });
            return;
          }
          if (msg.subtype === "success" && !msg.is_error) {
            finish(session, { ok: true, result: { reason: "completed", threadDisposition: "reusable", usage: msg.usage, costUsd: msg.total_cost_usd } });
          } else {
            const text = typeof msg.result === "string" ? msg.result : (msg.errors ?? []).join("; ");
            const kind = errorKind(msg.api_error_status, text);
            const reason = msg.subtype === "error_max_turns" ? "maxTurns" : "failed";
            finish(session, {
              ...failure(Object.assign(new Error(text || `Claude ended with ${msg.subtype}`), { name: "ClaudeResultError" }), kind),
              result: { reason, threadDisposition: kind === "session_lost" ? "broken" : "reusable" },
            });
          }
        }
      };

      const ensureQuery = async (session) => {
        if (session.query) return;
        const { query } = await sdk();
        session.inbox = new AsyncQueue();
        const base = { ...session.options.sdk, cwd: session.cwd, includePartialMessages: true, canUseTool: canUseTool(session), spawnClaudeCodeProcess: spawnProcess(session) };
        if (session.options.claudePath) base.pathToClaudeCodeExecutable = session.options.claudePath;
        if (session.resume) base.resume = session.resume;
        if (session.resumeAt) base.resumeSessionAt = session.resumeAt;
        const live = query({ prompt: session.inbox, options: base });
        session.query = live;
        session.resumeAt = undefined;
        session.pump = (async () => {
          try {
            for await (const msg of live) onMessage(session, msg);
            if (session.query === live) finish(session, failure(new Error("The engine stopped before answering."), "engine"));
          } catch (cause) {
            if (session.query !== live) return;
            const kind = errorKind(cause?.status, String(cause?.message));
            finish(session, failure(cause, kind));
          }
        })();
      };

      return {
        async handle(request) {
          switch (request.type) {
            case "open": {
              const { sessionId, cwd, options = {}, confinement } = request;
              let resume = request.operation === "resume" ? request.engineSessionId : undefined;
              if (request.operation === "fork") {
                const { forkSession } = await sdk();
                const forked = await forkSession(request.engineSessionId, { dir: cwd });
                resume = forked.sessionId;
              }
              let resumeAt;
              if (request.resumeAt !== undefined) {
                if (request.operation !== "resume" || typeof request.resumeAt !== "string" || !UUID.test(request.resumeAt)) {
                  throw invalidRequest("resumeAt needs operation resume and the uuid of an assistant message.");
                }
                const api = await sdk();
                if (typeof api.getSessionMessages !== "function") throw unsupportedSdkMethod("getSessionMessages");
                const known = await api.getSessionMessages(resume, { dir: cwd });
                const found = Array.isArray(known) && known.some((m) => m?.type === "assistant" && m.uuid === request.resumeAt && !m.parent_tool_use_id);
                if (!found) throw invalidRequest("resumeAt is not a top-level assistant message of this session.");
                resumeAt = request.resumeAt;
              }
              sessions.set(sessionId, { id: sessionId, cwd, options, confinement, resume, resumeAt, engineSessionId: resume, turnHeads: new Map(), run: null });
              return { engineSessionId: resume ?? null };
            }
            case "send": {
              const session = sessions.get(request.sessionId);
              if (!session) throw Object.assign(new Error("unknown session"), { name: "UnknownSession" });
              if (session.run) throw Object.assign(new Error("a run is already open on this session"), { name: "RunInProgress" });
              await ensureQuery(session);
              // The core names its prompt so it can tell the prompt's own turn from a leftover one in the engine's frames.
              const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.messageId ?? "") ? request.messageId : randomUUID();
              session.run = { sendId: request.id, turnId: `turn-${request.id}`, uuid, held: new Set(), done: false };
              session.inbox.push({ type: "user", message: { role: "user", content: request.message }, parent_tool_use_id: null, uuid: session.run.uuid });
              return { runId: `run-${request.id}`, engineSessionId: session.engineSessionId ?? undefined, turnId: session.run.turnId, messageId: uuid };
            }
            case "steer": {
              const session = [...sessions.values()].find((s) => s.run?.sendId === request.sendId);
              if (!session) return { outcome: "rejected", messageId: request.messageId };
              session.inbox.push({ type: "user", message: { role: "user", content: request.text }, parent_tool_use_id: null });
              return { outcome: "queued", messageId: request.messageId };
            }
            case "cancel": {
              const session = [...sessions.values()].find((s) => s.run?.sendId === request.sendId);
              if (!session) return { cancelled: false };
              const settled = new Promise((resolve) => (session.resultWaiter = resolve));
              for (const [id, p] of pendingPermissions) {
                if (p.sessionId === session.id) {
                  pendingPermissions.delete(id);
                  p.resolve({ behavior: "deny", message: "The run was cancelled." });
                }
              }
              await session.query.interrupt();
              await Promise.race([settled, new Promise((r) => setTimeout(r, session.options.cancelGraceMs ?? 5000))]);
              finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              return { cancelled: true };
            }
            case "respond": {
              const p = pendingPermissions.get(request.requestId);
              if (!p) {
                ctx.log({ type: "lateRespond", requestId: request.requestId });
                return null;
              }
              pendingPermissions.delete(request.requestId);
              const d = request.decision ?? {};
              // Once-only allow: no updatedPermissions, so the SDK never learns a session-wide rule.
              if (p.tool === "AskUserQuestion" && d.answers && typeof d.answers === "object") {
                p.resolve({ behavior: "allow", updatedInput: { ...p.input, answers: d.answers } });
                return null;
              }
              if (p.tool === "AskUserQuestion" && d.behavior === "allow") {
                p.resolve({ behavior: "deny", message: "The person did not answer." }); // a question is only answered with answers
                return null;
              }
              p.resolve(d.behavior === "allow" ? { behavior: "allow", updatedInput: p.input } : { behavior: "deny", message: d.message ?? "Denied." });
              return null;
            }
            case "stopTask": {
              const session = sessions.get(request.sessionId);
              if (!session?.query) return { stopped: false };
              await session.query.stopTask(request.taskId);
              return { stopped: true };
            }
            case "background": {
              const session = [...sessions.values()].find((s) => s.run?.sendId === request.sendId);
              if (!session?.query) return { backgrounded: false };
              return { backgrounded: await session.query.backgroundTasks(request.toolCallId) };
            }
            case "usage": {
              const session = sessions.get(request.sessionId);
              if (!session?.query) throw Object.assign(new Error("no live query for this session"), { name: "NoLiveQuery" });
              return await session.query.getContextUsage({ detail: "summary" });
            }
            case "models": {
              // A throwaway query with no prompt: it starts the engine, answers, and is closed.
              const { query } = await sdk();
              const inbox = new AsyncQueue();
              const probe = query({
                prompt: inbox,
                options: {
                  cwd: request.cwd ?? process.cwd(),
                  ...(request.claudePath ? { pathToClaudeCodeExecutable: request.claudePath } : {}),
                  spawnClaudeCodeProcess: spawnProcess({ confinement: request.confinement }),
                },
              });
              try {
                return await probe.supportedModels();
              } finally {
                inbox.close();
                probe.close?.();
              }
            }
            case "rollback": {
              const session = sessions.get(request.sessionId);
              if (!session) throw Object.assign(new Error("unknown session"), { name: "UnknownSession" });
              if (session.run) throw Object.assign(new Error("a run is already open on this session"), { name: "RunInProgress" });
              const head = session.turnHeads.get(request.toTurn);
              if (!head || !session.engineSessionId) return { ok: false };
              session.inbox?.close();
              session.query?.close?.();
              session.query = undefined;
              session.inbox = undefined;
              session.pump = undefined;
              session.resume = session.engineSessionId;
              session.resumeAt = head;
              // The engine's transcript still holds the rolled-back turns; the host forgets their heads.
              let after = false;
              for (const turnId of [...session.turnHeads.keys()]) {
                if (after) session.turnHeads.delete(turnId);
                else if (turnId === request.toTurn) after = true;
              }
              return { ok: true };
            }
            case "readHistory": {
              const session = sessions.get(request.sessionId);
              if (!session) throw Object.assign(new Error("unknown session"), { name: "UnknownSession" });
              if (!session.engineSessionId) throw hostError("NoEngineSession", "this session has no engine session yet");
              const api = await sdk();
              if (typeof api.getSessionMessages !== "function") throw unsupportedSdkMethod("getSessionMessages");
              return boundedHistory(await api.getSessionMessages(session.engineSessionId, { dir: session.cwd }));
            }
            case "readSubagent": {
              const session = sessions.get(request.sessionId);
              if (!session) throw Object.assign(new Error("unknown session"), { name: "UnknownSession" });
              if (!session.engineSessionId) throw hostError("NoEngineSession", "this session has no engine session yet");
              if (typeof request.agentId !== "string" || !SUBAGENT_ID.test(request.agentId)) {
                throw invalidRequest("agentId must be 1 to 128 letters, digits, underscores or hyphens.");
              }
              if (request.limit !== undefined && (!Number.isInteger(request.limit) || request.limit < 1 || request.limit > SUBAGENT_LIMIT_MAX)) {
                throw invalidRequest(`limit must be a positive integer no greater than ${SUBAGENT_LIMIT_MAX}.`);
              }
              const api = await sdk();
              if (typeof api.getSubagentMessages !== "function") throw unsupportedSdkMethod("getSubagentMessages");
              return boundedHistory(
                await api.getSubagentMessages(session.engineSessionId, request.agentId, {
                  dir: session.cwd,
                  ...(request.limit !== undefined ? { limit: request.limit } : {}),
                }),
              );
            }
            case "closeSession": {
              const session = sessions.get(request.sessionId);
              session?.inbox?.close();
              session?.query?.close?.();
              sessions.delete(request.sessionId);
              return null;
            }
            default:
              throw Object.assign(new Error(`unsupported request ${request.type}`), { name: "UnsupportedRequest" });
          }
        },
        async close() {
          for (const s of sessions.values()) {
            s.inbox?.close();
            s.query?.close?.();
          }
        },
      };
    },
  };
}
