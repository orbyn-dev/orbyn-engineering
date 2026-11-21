import { existsSync } from "node:fs";
import { realpath } from "node:fs/promises";
import { win32 } from "node:path";

// Muse Code on the host: the stdio client of @muse-code/sdk (MIT, bundled) over `muse serve` of the
// person's own `muse` binary (never bundled). One `muse serve` per open session, started under the core's
// confinement prefix and environment. Policy stays in Rust: sessions always ask (promptUnmatched, :ask-me)
// and every approval becomes a `permission` frame answered by `respond`, once-only. Ideas for the session
// framing, the approval choices and the run-end rules follow T3 Code's provider-muse (MIT, Copyright (c)
// 2026 T3 Tools Inc.); see test/fixtures/NOTICE. Nothing here has run against a real model turn yet; see the README.

export const SDK_VERSION = "1.4.6"; // keep equal to package.json (a test checks it)

/** Muse validates turn roots on Windows in verbatim form (T3: museVerbatimPath). */
export function verbatimPath(path) {
  if (path.startsWith("\\\\?\\")) return path;
  return path.startsWith("\\\\") ? `\\\\?\\UNC\\${path.slice(2)}` : `\\\\?\\${path}`;
}

const envValue = (env, name) => {
  const key = Object.keys(env).find((k) => k.toUpperCase() === name.toUpperCase());
  return key === undefined ? undefined : env[key];
};

/**
 * Windows launchers: resolve `muse` with PATH and PATHEXT, and run a .cmd or .bat under cmd.exe, because the
 * SDK spawns without a shell (T3: museLaunch). Elsewhere, and for a real executable, the command is unchanged.
 */
export function museLaunch(binary, env = process.env, { platform = process.platform, exists = existsSync } = {}) {
  if (platform !== "win32") return { command: binary, args: [] };
  const exts = (envValue(env, "PATHEXT") ?? ".COM;.EXE;.BAT;.CMD").split(";").filter(Boolean);
  const hasDir = /[\\/]/.test(binary);
  const dirs = hasDir ? [""] : (envValue(env, "PATH") ?? "").split(";").filter(Boolean);
  const names = win32.extname(binary) ? [binary, ...exts.map((e) => binary + e)] : exts.map((e) => binary + e);
  let resolved;
  for (const dir of dirs) {
    for (const name of names) {
      const candidate = dir === "" ? name : win32.join(dir, name);
      if (exists(candidate)) {
        resolved = candidate;
        break;
      }
    }
    if (resolved) break;
  }
  resolved ??= binary;
  if (!/\.(?:cmd|bat)$/i.test(resolved)) return { command: resolved, args: [] };
  const systemRoot = envValue(env, "SystemRoot");
  return {
    command: envValue(env, "ComSpec") ?? (systemRoot ? `${systemRoot}\\System32\\cmd.exe` : "cmd.exe"),
    // /d skips AutoRun; a bare @ keeps a quoted path with spaces intact under /c.
    args: ["/d", "/c", ...(/\s/.test(resolved) ? ["@", resolved] : [resolved.replace(/[()&<>@^|]/g, "^$&")])],
  };
}

const START_TIMEOUT_MS = 20_000;
const REQUEST_TIMEOUT_MS = 30_000;
const WAKE_GRACE_MS = 5_000;
const CANCEL_GRACE_MS = 5_000;
const ASK_MODE = "promptUnmatched";
const ASK_PROFILE = ":ask-me";

const owned = new WeakSet();
const hostError = (name, message, kind, cause) => {
  const error = Object.assign(new Error(message, cause ? { cause } : undefined), { name, kind });
  owned.add(error);
  return error;
};

function withTimeout(promise, ms, make) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => (timer = setTimeout(() => reject(make()), ms)))]).finally(() => clearTimeout(timer));
}

const AUTH = /not logged in|log ?in|unauthori[sz]ed|authentication|sign in|api key/i;
const LIMIT = /rate.?limit|usage limit|quota/i;

/** Maps a thrown SDK error to the protocol's closed `kind` set, without the SDK's own `kind` leaking into a cause. */
function museError(cause, fallback = "engine") {
  if (owned.has(cause)) return cause;
  const name = cause?.name;
  const text = String(cause?.message ?? cause);
  if (name === "MuseHostStartError") {
    if (cause.failure?.kind === "spawnFailed" && cause.failure.error?.code === "ENOENT") {
      return hostError("MuseNotFound", "Muse Code is not installed or not on PATH.", "engine");
    }
    const tail = cause.stderrTail?.findLast?.((line) => line.trim())?.trim();
    const kind = AUTH.test(`${text} ${tail ?? ""}`) ? "auth" : "engine";
    return hostError("MuseStartFailed", kind === "auth" ? "Sign in to Muse Code with `muse login`." : `Muse Code did not start${tail ? `: ${tail}` : "."}`, kind);
  }
  let kind = fallback;
  if (name === "MspError") {
    const k = cause.kind;
    if (k === "sessionNotFound" || k === "notFound" || k === "sessionNotLoaded") kind = "session_lost";
    else if (k === "invalidParams" || k === "invalidRequest" || k === "inputTooLarge") kind = "invalid_request";
    else if (k === "capabilityRequired" || k === "methodNotFound") kind = "unsupported";
    else if (k === "overloaded" || k === "backpressured") kind = "network";
    else if (k === "cancelled" || k === "interrupted") kind = "cancelled";
    else if (AUTH.test(text)) kind = "auth";
    else if (LIMIT.test(text)) kind = "rate_limit";
  } else if (AUTH.test(text)) kind = "auth";
  else if (LIMIT.test(text)) kind = "rate_limit";
  const error = Object.assign(new Error(text), { name: name ?? "Error", kind, ...(typeof cause?.code === "string" ? { code: cause.code } : {}) });
  owned.add(error);
  return error;
}

const retryFor = (kind, resetsAt) => (kind === "rate_limit" ? { why: "rate_limit", ...(resetsAt ? { resetsAt } : {}) } : kind === "network" ? { why: "network" } : undefined);

const usageOf = (u) =>
  u && typeof u === "object"
    ? { inputTokens: u.inputTokens, outputTokens: u.outputTokens, cachedInputTokens: u.cacheReadTokens ?? u.cachedTokens, reasoningTokens: u.reasoningTokens }
    : undefined;

const requirementKey = (approval) => JSON.stringify(approval?.currentRequirementId ?? null);

export function museFamily({ loadSdk, startTimeoutMs = START_TIMEOUT_MS, requestTimeoutMs = REQUEST_TIMEOUT_MS, wakeGraceMs = WAKE_GRACE_MS }) {
  return {
    name: "muse",
    sdk: { name: "@muse-code/sdk", version: SDK_VERSION },
    capabilities: ["steer"],
    create(ctx) {
      const sessions = new Map(); // host sessionId -> session
      const pending = new Map(); // requestId -> {session, kind, native}
      let counter = 0;
      let sdkPromise;
      const sdk = () => (sdkPromise ??= loadSdk());

      // Frames for a run wait until the send response is written, so a fast engine never lets a delta or
      // runDone overtake the response that announces the run.
      const emit = (run, frame) => (run.open ? ctx.send(frame) : run.buffer.push(frame));
      const sendId = (session) => (session.run ? { sendId: session.run.sendId } : {});

      const forget = (session, { keepChildren = false } = {}) => {
        for (const [id, p] of pending) {
          if (p.session !== session) continue;
          if (keepChildren && p.native.subagentOrigin !== undefined) continue;
          pending.delete(id);
        }
      };

      const finish = (session, frame) => {
        const run = session.run;
        if (!run || run.done) return;
        run.done = true;
        clearTimeout(run.grace);
        session.run = null;
        forget(session, { keepChildren: true });
        emit(run, { type: "runDone", sendId: run.sendId, ...frame });
        run.ended?.();
      };

      const failFrame = (error, extra = {}, resetsAt) => ({
        ok: false,
        result: { reason: "failed", threadDisposition: "reusable", ...(retryFor(error.kind, resetsAt) ? { retry: retryFor(error.kind, resetsAt) } : {}), ...extra },
        error: { method: "send", cause: { name: error.name ?? "Error", message: error.message ?? String(error), kind: error.kind, ...(error.code ? { code: error.code } : {}) } },
      });

      const resultFrame = (turn, run) => {
        const usage = usageOf(turn.usage) ?? run.usage;
        if (turn.terminal === "completed") return { ok: true, result: { reason: "completed", threadDisposition: "reusable", ...(usage ? { usage } : {}) } };
        if (turn.terminal === "cancelled") return { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } };
        const message = turn.error?.message ?? turn.reason ?? "Muse ended the run with an error.";
        const resetsMs = turn.error?.quota?.resetsAtMs;
        const kind = resetsMs !== undefined || LIMIT.test(message) ? "rate_limit" : AUTH.test(message) ? "auth" : "engine";
        const error = Object.assign(new Error(message), { name: "MuseTurnError", kind });
        return failFrame(error, {}, resetsMs !== undefined ? Math.floor(resetsMs / 1000) : undefined);
      };

      const lose = (session, cause) => {
        if (session.closing || session.broken) return;
        session.broken = true;
        const error = hostError("MuseServerLost", "Muse stopped. Your work in the project is kept; send your message again.", "engine");
        ctx.log({ type: "museLost", message: String(cause?.message ?? cause) });
        finish(session, failFrame(error, { threadDisposition: "broken" }));
        forget(session);
        void session.host?.close().catch(() => {});
      };

      const background = (session, event) => {
        const frame = { type: "background", sessionId: session.id, event };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };

      // Workflows are the background work the recordings cover cleanly: item/started (inProgress) to
      // item/completed (terminal). A running one holds the run; see settleTurn.
      const trackWorkflow = (session, item) => {
        const known = session.workflows.get(item.itemId);
        const running = item.status === "inProgress";
        if (!known && running) {
          session.workflows.set(item.itemId, { running: true });
          background(session, { type: "started", id: item.itemId, kind: "workflow", ...(item.entryId ? { entryId: item.entryId } : {}), ...(item.workflowRunId ? { workflowRunId: item.workflowRunId } : {}) });
        } else if (known?.running && !running) {
          known.running = false;
          background(session, { type: "ended", id: item.itemId, kind: "workflow", status: item.status, ...(item.workflowRunId ? { workflowRunId: item.workflowRunId } : {}) });
          onBackgroundSettled(session);
        }
      };
      const workflowsRunning = (session) => [...session.workflows.values()].some((w) => w.running);

      const settleTurn = (session, run, turn) => {
        const own = turn.turnId === run.turnId;
        if (!own && !run.held) return; // a leftover turn the engine started for itself: shown, never the end
        if (own) run.ownResult = turn;
        if (workflowsRunning(session)) {
          if (!run.held) ctx.log({ type: "runHeld", why: "workflow" });
          run.held = true;
          run.heldResult = turn;
          return;
        }
        finish(session, resultFrame(turn, run));
      };

      // The workflow ended: Muse then starts a follow-up turn for its result. Wait a short time for it, then end.
      const onBackgroundSettled = (session) => {
        const run = session.run;
        if (!run?.held || workflowsRunning(session)) return;
        clearTimeout(run.grace);
        run.grace = setTimeout(() => {
          if (session.run === run) finish(session, resultFrame(run.heldResult ?? run.ownResult, run));
        }, wakeGraceMs);
        run.grace.unref?.();
      };

      const publishApproval = (session, params) => {
        const known = [...pending.entries()].find(([, p]) => p.kind === "approval" && p.native.approvalId === params.approvalId);
        if (known) {
          const [, entry] = known;
          const changed = requirementKey(entry.native) !== requirementKey(params);
          entry.native = { ...entry.native, ...params };
          // A multi-stage approval asks again under the same id once a stage is answered.
          if (!changed || !entry.answered) return;
          entry.answered = false;
          entry.requestId = `p-${++counter}`;
          pending.delete(known[0]);
          pending.set(entry.requestId, entry);
          return emitAsk(session, entry);
        }
        if (params.sessionId !== session.engineSessionId) return;
        const entry = { session, kind: "approval", native: params, requestId: `p-${++counter}`, answered: false };
        pending.set(entry.requestId, entry);
        emitAsk(session, entry);
      };

      const emitAsk = (session, entry) => {
        const p = entry.native;
        const frame = { type: "permission", ...sendId(session), requestId: entry.requestId, tool: p.toolName ?? p.subject?.kind, input: p, options: p.availableChoices };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };

      const publishQuestion = (session, params) => {
        if ([...pending.values()].some((p) => p.kind === "question" && p.native.userInputId === params.userInputId)) return;
        const entry = { session, kind: "question", native: params, requestId: `q-${++counter}` };
        pending.set(entry.requestId, entry);
        const frame = { type: "question", ...sendId(session), requestId: entry.requestId, tool: "request_user_input", input: params };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };

      const handleNotification = (session, notification) => {
        const { method, params = {} } = notification;
        const run = session.run;
        if (params.sessionId !== session.engineSessionId) return;
        if (method === "view/gap") return lose(session, new Error("Muse lost part of its event stream."));
        if (method === "session/closed") {
          if (!session.closing) lose(session, new Error(`Muse closed the session (${params.reason ?? "unknown"}).`));
          return;
        }
        if (params.item && /^item\//.test(method) && params.item.kind === "workflow") trackWorkflow(session, params.item);
        if (method === "approval/requested" || (method === "approval/updated" && [...pending.values()].some((p) => p.kind === "approval" && p.native.approvalId === params.approvalId))) publishApproval(session, params);
        else if (method === "userInput/requested") publishQuestion(session, params);
        else if (method === "approval/resolved" || method === "userInput/settled") {
          for (const [id, p] of pending) if (p.session === session && (p.native.approvalId === params.approvalId || p.native.userInputId === params.userInputId) && (params.approvalId ?? params.userInputId) !== undefined) pending.delete(id);
        }
        if (!run) return;
        if (method === "session/tokenUsage" && params.usage) {
          const u = (run.usage ??= { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0, reasoningTokens: 0 });
          u.inputTokens += params.promptTokens ?? 0;
          u.outputTokens += params.usage.outputTokens ?? 0;
          u.cachedInputTokens += params.usage.cacheReadTokens ?? params.usage.cachedTokens ?? 0;
          u.reasoningTokens += params.usage.reasoningTokens ?? 0;
        }
        if (method === "turn/started" && params.turnId !== run.turnId) clearTimeout(run.grace);
        emit(run, { type: "delta", sendId: run.sendId, update: { method, params } });
        if (method === "turn/completed") settleTurn(session, run, params);
      };

      // Notifications that arrive before turn/start has answered cannot be matched to the run's turn yet.
      const onNotification = (session, notification) => {
        const run = session.run;
        if (run && run.turnId === undefined && !run.done) return void run.early.push(notification);
        handleNotification(session, notification);
      };

      const call = async (session, method, params, options) => {
        try {
          return await withTimeout(session.host.connection.command(method, { sessionId: session.engineSessionId, ...params }, { maxAttempts: 1, ...options }), requestTimeoutMs, () => hostError("MuseTimeout", `Muse ${method} timed out; its process was stopped.`, "network"));
        } catch (cause) {
          const error = museError(cause);
          if (error.name === "MuseTimeout") lose(session, error);
          throw error;
        }
      };

      const sessionOf = (id) => {
        const session = sessions.get(id);
        if (!session) throw hostError("UnknownSession", "unknown session", "invalid_request");
        return session;
      };
      const runOf = (id) => [...sessions.values()].find((s) => s.run?.sendId === id);

      const startProcess = async (request, spawn) => {
        const { cwd, options = {}, confinement } = request;
        const prefix = confinement?.argvPrefix;
        if (prefix !== undefined && !(Array.isArray(prefix) && prefix.length > 0 && prefix.every((p) => typeof p === "string" && p.length > 0))) {
          throw hostError("InvalidConfinement", "confinement.argvPrefix must be a list of non-empty strings.", "invalid_request");
        }
        const fromCore = confinement?.env !== undefined;
        const env = {};
        for (const [k, v] of Object.entries(fromCore ? confinement.env : process.env)) {
          if (typeof v !== "string") continue;
          if (!fromCore && k.toUpperCase() === "META_API_KEY") continue; // the core, not this host, supplies credentials
          env[k] = v;
        }
        if (env.MUSE_NO_AUTO_UPDATE === undefined) {
          env.MUSE_NO_AUTO_UPDATE = "1";
          ctx.log({ type: "envAdded", keys: ["MUSE_NO_AUTO_UPDATE"] });
        }
        const serve = ["serve", "--trust-workspace", ...(options.disableSandbox === true ? ["--disable-sandbox"] : [])];
        // Under a confinement prefix the prefix is the program; the Windows launcher applies to a bare muse.
        const launch = prefix === undefined ? museLaunch(options.musePath ?? "muse", env) : { command: options.musePath ?? "muse", args: [] };
        const [command, ...args] = [...(prefix ?? []), launch.command, ...launch.args, ...serve];
        const handshake = spawn({
          command,
          args,
          cwd: confinement?.cwd ?? cwd,
          env,
          shutdownTimeoutMs: 10_000,
          onStderr: (chunk) => process.stderr.write(chunk),
        });
        try {
          const host = await withTimeout(
            handshake.initialize({ clientInfo: { name: "orbyn", title: "Orbyn", version: "1" }, capabilities: { requestedCapabilities: [] } }),
            startTimeoutMs,
            () => hostError("MuseNotReady", "Timed out waiting for Muse Code to start.", "network"),
          );
          if (host.initializeResult?.schema?.version !== 1) throw hostError("MuseProtocol", "Muse Code speaks a protocol this app does not know. Update Muse Code.", "engine");
          return host;
        } catch (cause) {
          await Promise.resolve(handshake.close()).catch(() => {});
          throw museError(cause);
        }
      };

      return {
        async handle(request) {
          switch (request.type) {
            case "open": {
              const { sessionId, cwd, options = {}, operation } = request;
              if (sessions.has(sessionId)) throw hostError("SessionOpen", "this session is already open", "invalid_request");
              if (operation === "fork") throw hostError("UnsupportedFork", "Muse Code cannot fork a session yet.", "unsupported");
              if (options.serverUrl !== undefined) throw hostError("UnsupportedServer", "Attaching to a running Muse server is not supported.", "unsupported");
              if (operation === "resume" && typeof request.engineSessionId !== "string") throw hostError("InvalidOpen", "resume needs engineSessionId.", "invalid_request");
              const { spawnMspConnection } = await sdk();
              const host = await startProcess(request, spawnMspConnection);
              const session = { id: sessionId, cwd, options, host, engineSessionId: undefined, run: null, workflows: new Map(), closing: false, broken: false };
              host.connection.onNotification((n) => onNotification(session, n));
              host.connection.onProtocolError((e) => ctx.log({ type: "protocolError", message: String(e?.message) }));
              host.connection.onServerRequest(async (r) => {
                if (r.method === "approval/request" || r.method === "userInput/request") return {};
                throw new Error(`Unsupported Muse server request: ${r.method}`);
              });
              void host.connection.closed.then(() => lose(session, new Error("The Muse connection closed.")));
              void Promise.resolve(host.exited).then((exit) => lose(session, new Error(`Muse exited (${exit?.code ?? exit?.signal}).`)));
              try {
                const workspaceRoot = await realpath(cwd).catch(() => cwd);
                session.root = workspaceRoot;
                if (operation === "resume") {
                  session.engineSessionId = request.engineSessionId;
                  const result = await call(session, "session/resume", { excludeItems: true, permissionProfile: ASK_PROFILE });
                  if (result.session?.sessionId !== session.engineSessionId) throw hostError("MuseSession", "Muse returned a different session.", "engine");
                  await call(session, "session/setApprovalMode", { mode: ASK_MODE });
                } else {
                  session.engineSessionId = host.connection.mintCommandId();
                  const result = await call(session, "session/start", {
                    workspaceRoot,
                    ...(options.model ? { modelId: options.model } : {}),
                    providerId: "meta",
                    approvalMode: ASK_MODE,
                    permissionProfile: ASK_PROFILE,
                  });
                  if (result.session?.sessionId !== session.engineSessionId) throw hostError("MuseSession", "Muse returned a different session.", "engine");
                }
              } catch (cause) {
                session.closing = true;
                await Promise.resolve(host.close()).catch(() => {});
                throw museError(cause);
              }
              sessions.set(sessionId, session);
              return { engineSessionId: session.engineSessionId };
            }
            case "send": {
              const session = sessionOf(request.sessionId);
              if (session.broken) throw hostError("MuseServerLost", "Muse stopped. Open the session again.", "session_lost");
              if (session.run) throw hostError("RunInProgress", "a run is already open on this session", "invalid_request");
              const message = request.message;
              const text = typeof message === "string" ? message : message?.text;
              if (typeof text !== "string") throw hostError("InvalidMessage", "message must be text.", "invalid_request");
              if (Array.isArray(message?.attachments) && message.attachments.length > 0) throw hostError("UnsupportedAttachment", "Attachments are not supported for Muse yet.", "unsupported");
              const run = { sendId: request.id, open: false, buffer: [], early: [], done: false, turnId: undefined, held: false };
              session.run = run;
              const effort = request.options?.effort ?? session.options.effort;
              let accepted;
              try {
                accepted = await call(session, "turn/start", { input: [{ type: "text", text }], workspaceRoots: [process.platform === "win32" ? verbatimPath(session.root) : session.root], ifBusy: "queue", ...(effort ? { reasoningEffort: effort } : {}) }, { maxAttempts: 3 });
              } catch (cause) {
                if (session.run === run) session.run = null;
                throw museError(cause);
              }
              run.turnId = accepted.turnId;
              for (const n of run.early.splice(0)) handleNotification(session, n);
              setImmediate(() => {
                run.open = true;
                for (const frame of run.buffer.splice(0)) ctx.send(frame);
              });
              return { runId: `run-${request.id}`, engineSessionId: session.engineSessionId, turnId: run.turnId };
            }
            case "steer": {
              const session = runOf(request.sendId);
              if (!session || session.broken || session.run.held) return { outcome: "rejected", messageId: request.messageId };
              try {
                await call(session, "turn/steer", { expectedTurnId: session.run.turnId, input: [{ type: "text", text: request.text }] });
                return { outcome: "accepted", messageId: request.messageId };
              } catch (cause) {
                ctx.log({ type: "steerRejected", kind: museError(cause).kind });
                return { outcome: "rejected", messageId: request.messageId };
              }
            }
            case "cancel": {
              const session = runOf(request.sendId);
              if (!session || session.broken) return { cancelled: false };
              const run = session.run;
              if (run.held) {
                // The turn is over; a running workflow is not stopped by an interrupt (stopTask is Planned).
                ctx.log({ type: "cancelHeld", workflows: [...session.workflows].filter(([, w]) => w.running).map(([id]) => id) });
                finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
                return { cancelled: true };
              }
              const ended = new Promise((resolve) => (run.ended = resolve));
              try {
                await call(session, "turn/interrupt", { turnId: run.turnId });
              } catch (cause) {
                throw museError(cause);
              }
              await Promise.race([ended, new Promise((r) => setTimeout(r, session.options.cancelGraceMs ?? CANCEL_GRACE_MS).unref?.())]);
              finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              return { cancelled: true };
            }
            case "respond": {
              const entry = pending.get(request.requestId);
              if (!entry) {
                ctx.log({ type: "lateRespond", requestId: request.requestId });
                return null;
              }
              const { session } = entry;
              const d = request.decision ?? {};
              if (entry.kind === "question") {
                if (!d.answers) throw hostError("UnsupportedSkip", "Muse cannot skip a question; answer it or cancel the run.", "unsupported");
                const answers = [];
                for (const q of entry.native.questions) {
                  const raw = d.answers[q.question];
                  if (typeof raw !== "string" || raw.trim() === "") throw hostError("InvalidAnswer", `Answer the question: ${q.question}`, "invalid_request");
                  const labels = q.options.map((o) => o.label);
                  const values = labels.includes(raw) ? [raw] : q.selection?.mode === "multiple" ? raw.split(",").map((v) => v.trim()).filter(Boolean) : [raw];
                  const selected = values.filter((v) => labels.includes(v));
                  const free = values.filter((v) => !labels.includes(v)).join("\n");
                  answers.push({ questionId: q.id, ...(selected.length ? (q.selection?.mode === "multiple" ? { selectedLabels: selected } : { selectedLabel: selected[0] }) : {}), ...(free ? (selected.length ? { note: free } : { freeText: free }) : {}) });
                }
                pending.delete(request.requestId);
                await call(session, "userInput/answer", { userInputId: entry.native.userInputId, answers });
                return null;
              }
              const choices = entry.native.availableChoices ?? [];
              // Once-only: allow is the "approved" choice with scope "once". An engine that offers only a wider
              // allow gets a refusal, and a persistent deny rule is never picked.
              const allow = choices.find((c) => c.decision === "approved" && c.scope === "once");
              const deny = choices.find((c) => c.decision === "denied" && c.scope === "once") ?? choices.find((c) => c.decision === "abort" && c.scope === "once");
              const choice = d.behavior === "allow" && allow ? allow : deny;
              if (!choice) {
                // No way to refuse: stop the turn that asked.
                pending.delete(request.requestId);
                if (entry.native.turnId) await call(session, "turn/interrupt", { turnId: entry.native.turnId });
                return null;
              }
              entry.answered = true;
              try {
                await call(session, "approval/decide", { approvalId: entry.native.approvalId, requirementId: entry.native.currentRequirementId, choiceId: choice.choiceId });
              } catch (cause) {
                entry.answered = false;
                throw museError(cause);
              }
              return null;
            }
            case "closeSession": {
              const session = sessions.get(request.sessionId);
              if (!session) return null;
              sessions.delete(request.sessionId);
              if (session.run && !session.broken) {
                const run = session.run;
                if (!run.held && run.turnId !== undefined) await call(session, "turn/interrupt", { turnId: run.turnId }).catch(() => {});
                finish(session, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
              }
              session.closing = true;
              forget(session);
              await Promise.resolve(session.host.close()).catch(() => {});
              return null;
            }
            default:
              throw hostError("UnsupportedRequest", `unsupported request ${request.type}`, "unsupported");
          }
        },
        async close() {
          for (const session of sessions.values()) {
            session.closing = true;
            clearTimeout(session.run?.grace);
            await Promise.resolve(session.host.close()).catch(() => {});
          }
          sessions.clear();
        },
      };
    },
  };
}
