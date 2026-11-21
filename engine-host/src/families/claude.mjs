import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { AsyncQueue } from "../asyncQueue.mjs";

// Claude on the host: query() from @anthropic-ai/claude-agent-sdk over the person's own `claude`.
// Policy stays in Rust: every tool request becomes a `permission` frame. The SDK is not bundled;
// see the README (Anthropic terms gate). Nothing here has run against the real SDK yet.

const EXTRA_ENV_PREFIXES = ["CLAUDE_CODE_", "CLAUDE_AGENT_SDK_"];
const TERMINAL_TASK_STATUSES = new Set(["completed", "failed", "killed", "stopped", "cancelled"]);

function errorKind(status, text = "") {
  if (status === 401 || status === 403 || /not logged in|authenticat|oauth|\/login/i.test(text)) return "auth";
  if (status === 429) return "rate_limit";
  if (status === 529 || status === 503) return "network";
  if (/No conversation found/i.test(text)) return "session_lost";
  return "engine";
}

export function claudeFamily({ loadSdk }) {
  return {
    name: "claude",
    sdk: { name: "@anthropic-ai/claude-agent-sdk", version: process.env.ORBYN_CLAUDE_SDK_VERSION ?? "unknown" },
    capabilities: ["steer", "fork", "models", "usage", "stopTask", "background"],
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
          if (!(k in env) && EXTRA_ENV_PREFIXES.some((p) => k.startsWith(p))) {
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
        if (run) ctx.send({ type: "delta", sendId: run.sendId, update: msg });
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
        session.query = query({ prompt: session.inbox, options: base });
        session.pump = (async () => {
          try {
            for await (const msg of session.query) onMessage(session, msg);
            finish(session, failure(new Error("The engine stopped before answering."), "engine"));
          } catch (cause) {
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
              sessions.set(sessionId, { id: sessionId, cwd, options, confinement, resume, engineSessionId: resume, run: null });
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
