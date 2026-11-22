import { spawn as spawnProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { win32 } from "node:path";

// Codex on the host: a plain-JS JSON-RPC client for `codex app-server` of the person's own `codex`
// binary (never bundled), one app-server per distinct confinement. The session flow, approvals,
// rollback, token accounting and usage shapes follow T3 Code's effect-codex-app-server client,
// CodexProvider.ts, CodexThreadRevert.ts, CodexTurnTokenUsage.ts, codexUsageLimits.ts,
// CodexMcpElicitation.ts, CodexDeveloperInstructions.ts and CodexAdapterV2.ts (MIT, Copyright (c)
// 2026 T3 Tools Inc., commit 9168ca73d02003557725430b515b44e1551544af); see test/fixtures/NOTICE. Policy stays in Rust: every approval is a
// `permission` frame, every user-input or elicitation question a `question` frame, and this host only
// ever answers once, never for a session. Nothing here has run against a real Codex turn yet; see the
// README.

export const SDK_VERSION = "0.156.1"; // the app-server version the T3 recordings were made with
const CLIENT_NAME = "Orbyn";
const APP_SERVER_ARGS = ["app-server", "-c", "thread_unload_delay_secs=0"];
const START_TIMEOUT_MS = 20_000;
const REQUEST_TIMEOUT_MS = 60_000;
const WAKE_GRACE_MS = 5_000;
const HOLD_LIMIT_MS = 30 * 60_000;
const ASK_POLICY = "on-request";
const READ_ONLY = "read-only";
const WORKSPACE_WRITE = "workspace-write";
const APPROVAL_METHODS = new Set([
  "item/commandExecution/requestApproval",
  "item/fileChange/requestApproval",
  "item/permissions/requestApproval",
  "applyPatchApproval",
  "execCommandApproval",
  "mcpServer/elicitation/request",
]);

const defaultSpawn = (options) =>
  spawnProcess(options.command, options.args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });

const envValue = (env, name) => {
  const key = Object.keys(env).find((candidate) => candidate.toUpperCase() === name.toUpperCase());
  return key === undefined ? undefined : env[key];
};

/**
 * Windows launchers: resolve `codex` with PATH and PATHEXT, and run a .cmd or .bat under cmd.exe,
 * because app-server is spawned without a shell. Elsewhere, and for a real executable, unchanged.
 */
export function codexLaunch(binary, env = process.env, { platform = process.platform, exists = existsSync } = {}) {
  if (platform !== "win32") return { command: binary, args: [] };
  const exts = (envValue(env, "PATHEXT") ?? ".COM;.EXE;.BAT;.CMD").split(";").filter(Boolean);
  const hasDir = /[\\/]/.test(binary);
  const dirs = hasDir ? [""] : (envValue(env, "PATH") ?? "").split(";").filter(Boolean);
  const names = win32.extname(binary) ? [binary, ...exts.map((ext) => binary + ext)] : exts.map((ext) => binary + ext);
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

const owned = new WeakSet();
const hostError = (name, message, kind, cause) => {
  const error = Object.assign(new Error(message, cause ? { cause } : undefined), { name, kind });
  owned.add(error);
  return error;
};

// T3 CodexManagedErrors: ChatGPT subscription-sharing failures arrive as codes or prose, and the
// usage-limit ones are retryable limits rather than engine failures.
const MANAGED_ERRORS = {
  invalid_encrypted_content: "Codex could not verify this session's encrypted conversation history. Send another message to continue in a fresh session with the visible conversation history.",
  subscription_sharing_v2_user_not_eligible: "ChatGPT sharing is unavailable for this account or workspace. Use another provider or check its sharing policy.",
  subscription_sharing_usage_limit_exceeded: "Your ChatGPT usage limit was reached. Check ChatGPT Usage settings for your available allowance.",
  subscription_sharing_workplace_individual_usage_limit_exceeded: "You reached your workspace's limit for using ChatGPT in other apps. Ask your workspace admin to increase it.",
  subscription_sharing_usage_unavailable: "ChatGPT usage is temporarily unavailable. Try again shortly.",
  subscription_sharing_unsupported_capability: "Codex used a feature that ChatGPT sharing does not support. Use another provider for this request.",
  subscription_sharing_v2_client_not_enabled: "This app is not enabled for this ChatGPT connection. Use your existing CLI or another provider.",
  subscription_sharing_v2_route_not_supported: "ChatGPT does not support this request route.",
  subscription_sharing_v2_invalid_user: "ChatGPT could not validate this connection. Check the selected account and sharing permissions.",
  subscription_sharing_v2_user_unavailable: "ChatGPT is temporarily unavailable. Try again shortly.",
  chatpass_v2_scope_not_authorized: "This ChatGPT grant does not authorize the request. Check the connection's sharing permissions.",
  chatpass_v2_invalid_authorization_context: "ChatGPT could not authorize this connection. Check the client and sharing permissions.",
};
const MANAGED_USAGE_LIMITS = new Set(["subscription_sharing_usage_limit_exceeded", "subscription_sharing_workplace_individual_usage_limit_exceeded"]);
const MANAGED_ALIASES = {
  subscription_sharing_user_not_eligible: "subscription_sharing_v2_user_not_eligible",
  subscription_sharing_route_not_supported: "subscription_sharing_v2_route_not_supported",
  subscription_sharing_invalid_user: "subscription_sharing_v2_invalid_user",
  subscription_sharing_user_unavailable: "subscription_sharing_v2_user_unavailable",
};

function classifyManagedError(text) {
  const code = Object.keys(MANAGED_ERRORS).find((candidate) => text.includes(candidate));
  const aliased = code === undefined ? Object.keys(MANAGED_ALIASES).find((candidate) => text.includes(candidate)) : undefined;
  const resolved = code ?? (aliased === undefined ? undefined : MANAGED_ALIASES[aliased]);
  if (resolved !== undefined) return { code: resolved, message: MANAGED_ERRORS[resolved], usageLimit: MANAGED_USAGE_LIMITS.has(resolved) };
  if (text.includes("has reached their Subscription Sharing usage limit")) {
    return { code: "subscription_sharing_usage_limit_exceeded", message: MANAGED_ERRORS.subscription_sharing_usage_limit_exceeded, usageLimit: true };
  }
  return undefined;
}

const AUTH = /not logged in|log ?in|unauthori[sz]ed|authentication|sign in|api key|\b401\b/i;
const LIMIT = /rate.?limit|usage limit|quota|too many requests|\b429\b|insufficient_quota/i;
const NETWORK = /econnreset|econnrefused|enotfound|etimedout|socket hang up|fetch failed|network|disconnected|responseStream/i;
const OVERLOAD = /overload|too many|unavailable|\b50[234]\b|\b529\b/i;
const SESSION_LOST = /thread not found|session not found|no rollout|not loaded|unknown thread|thread .* (?:gone|missing)/i;

/** Maps any error crossing the Codex client to the protocol's closed `kind` set. */
export function mapCodexError(cause, fallback = "engine") {
  if (owned.has(cause)) return cause;
  const name = cause?.name ?? "Error";
  const text = String(cause?.message ?? cause);
  const code = cause?.code;
  let kind = fallback;
  let reason;
  if (code === "ENOENT") return hostError("CodexNotFound", "Codex is not installed or not on PATH.", "engine", cause);
  const managed = classifyManagedError(text);
  if (managed) {
    const error = hostError("CodexManagedError", managed.message, managed.usageLimit ? "rate_limit" : "engine");
    error.code = managed.code;
    return error;
  }
  if (name === "CodexProcessExitedError") kind = "engine";
  else if (name === "CodexTransportError" || name === "CodexTimeout") kind = "network";
  else if (SESSION_LOST.test(text)) kind = "session_lost";
  else if (AUTH.test(text) || code === 401) kind = "auth";
  else if (LIMIT.test(text) || code === 429) kind = "rate_limit";
  else if (OVERLOAD.test(text) || code === -32001) kind = "network";
  else if (NETWORK.test(text) || code === "ECONNRESET") kind = "network";
  else if (code === -32601 || /method not found/i.test(text)) kind = "unsupported";
  else if (code === -32600 || code === -32602 || /invalid (?:params|request|payload)/i.test(text)) kind = "invalid_request";
  else if (/refus(?:ed|al)|rejected/i.test(text)) {
    kind = "engine";
    reason = "refused";
  } else if (/max(?:imum)? turns/i.test(text)) {
    kind = "engine";
    reason = "maxTurns";
  }
  const error = Object.assign(new Error(text), {
    name,
    kind,
    ...(typeof code === "number" ? { code } : {}),
    ...(reason ? { reason } : {}),
  });
  owned.add(error);
  return error;
}

const RESET_TEXT = /[Tt]ry again at (?:([A-Z][a-z]{2}) (\d{1,2})(?:st|nd|rd|th), (\d{4}) )?(\d{1,2}):(\d{2}) ([AP]M)\./;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Codex prints a rate limit's reset in its own local time ("Try again at 3:37 PM."). */
function codexMessageResetAt(message, now = new Date()) {
  if (typeof message !== "string") return undefined;
  const match = RESET_TEXT.exec(message);
  if (!match) return undefined;
  const [, month, day, year, hour, minute, meridiem] = match;
  const monthIndex = month === undefined ? undefined : MONTHS.indexOf(month);
  if (monthIndex === -1) return undefined;
  const date = new Date(
    monthIndex === undefined ? now.getFullYear() : Number(year),
    monthIndex === undefined ? now.getMonth() : monthIndex,
    monthIndex === undefined ? now.getDate() : Number(day),
    (Number(hour) % 12) + (meridiem === "PM" ? 12 : 0),
    Number(minute) + 1,
    0,
    0,
  );
  if (Number.isNaN(date.getTime()) || date.getTime() <= now.getTime()) return undefined;
  return Math.floor(date.getTime() / 1000);
}

function codexErrorInfo(value) {
  if (typeof value === "string") return { code: value };
  if (typeof value !== "object" || value === null) return {};
  const [code] = Object.keys(value);
  const detail = code === undefined ? undefined : value[code];
  const status = typeof detail === "object" && detail !== null && typeof detail.httpStatusCode === "number" ? detail.httpStatusCode : undefined;
  return { code, status };
}

/** The turn-level error, including Codex's structured `codexErrorInfo`, as a protocol kind. */
function codexTurnError(native) {
  const message = native?.message ?? "Codex ended the turn with an error.";
  const { code, status } = codexErrorInfo(native?.codexErrorInfo);
  let error = mapCodexError(Object.assign(new Error(message), { name: "CodexTurnError" }));
  if (code === "usageLimitExceeded" || code === "rateLimitExceeded" || status === 429) {
    error = hostError("CodexTurnError", message, "rate_limit");
  } else if (code === "unauthorized" || code === "notLoggedIn" || status === 401) {
    error = hostError("CodexTurnError", message, "auth");
  } else if (typeof status === "number" && status >= 500) {
    error = hostError("CodexTurnError", message, "network");
  } else if (typeof status === "number" && status >= 400) {
    error = hostError("CodexTurnError", message, "invalid_request");
  } else if (code !== undefined && /connection|disconnected|failed/i.test(code)) {
    error = hostError("CodexTurnError", message, "network");
  }
  if (error.kind === "rate_limit") {
    const resetsAt = typeof native?.resetsAt === "number" ? native.resetsAt : codexMessageResetAt(message);
    if (resetsAt !== undefined) error.resetsAt = resetsAt;
  }
  if (/max(?:imum)? turns/i.test(message)) error.reason = "maxTurns";
  return error;
}

const retryFor = (error) => {
  if (error.kind === "rate_limit") {
    const seconds = error.resetsAt;
    return { why: "rate_limit", ...(typeof seconds === "number" && seconds > 0 ? { resetsAt: seconds } : {}) };
  }
  if (error.kind === "network") return { why: OVERLOAD.test(error.message) ? "overloaded" : "network" };
  return undefined;
};

const finite = (number) => typeof number === "number" && Number.isFinite(number);
const clampPercent = (number) => Math.min(100, Math.max(0, number));

function reasoningEffortLabel(effort) {
  return { none: "None", minimal: "Minimal", low: "Low", medium: "Medium", high: "High", xhigh: "Extra High", max: "Max", ultra: "Ultra" }[effort] ?? effort;
}

export function formatCodexModelName(name) {
  return name.replace(/^gpt/i, "GPT").replace(/-([a-z])/g, (_, letter) => `-${letter.toUpperCase()}`);
}

function codexDefaultReasoningEffortOverride(model) {
  return model === "gpt-6-astra" ? "medium" : undefined;
}

/** model/list as `{models}` in the host's model shape (muse's), one entry per Codex model. */
export function codexModels(response) {
  const data = Array.isArray(response) ? response : (response?.data ?? []);
  const seen = new Set();
  const models = [];
  for (const model of data) {
    if (!model || typeof model.model !== "string" || seen.has(model.model)) continue;
    seen.add(model.model);
    const preferred = codexDefaultReasoningEffortOverride(model.model) ?? model.defaultReasoningEffort;
    const efforts = (Array.isArray(model.supportedReasoningEfforts) ? model.supportedReasoningEfforts : [])
      .map((option) => option?.reasoningEffort)
      .filter((effort) => typeof effort === "string")
      .map((effort) => ({ id: effort, label: reasoningEffortLabel(effort), ...(effort === preferred ? { isDefault: true } : {}) }));
    models.push({
      id: model.model,
      name: typeof model.displayName === "string" && model.displayName !== "" ? formatCodexModelName(model.displayName) : formatCodexModelName(model.model),
      isDefault: model.isDefault === true,
      efforts,
      ...(efforts.some((effort) => effort.isDefault) ? { defaultEffort: preferred } : {}),
    });
  }
  return { models };
}

export function codexPlanLabel(planType) {
  switch (planType) {
    case "free":
      return "ChatGPT Free Subscription";
    case "go":
      return "ChatGPT Go Subscription";
    case "plus":
      return "ChatGPT Plus Subscription";
    case "pro":
      return "ChatGPT Pro 200 Subscription";
    case "prolite":
      return "ChatGPT Pro 100 Subscription";
    case "promax":
      return "ChatGPT Pro 500 Subscription";
    case "self_serve_business_prolite":
      return "ChatGPT Business Premium Subscription";
    case "team":
    case "self_serve_business_usage_based":
    case "business":
      return "ChatGPT Business Subscription";
    case "ent26":
    case "enterprise_cbp_automation":
    case "enterprise_cbp_usage_based":
    case "enterprise":
      return "ChatGPT Enterprise Subscription";
    case "edu":
    case "edu_plus":
    case "edu_pro":
      return "ChatGPT Edu Subscription";
    case "unknown":
      return "ChatGPT Subscription";
    default:
      return undefined;
  }
}

const SESSION_MINS = 5 * 60;
const WEEK_MINS = 7 * 24 * 60;
const MONTH_MINS = 30 * 24 * 60;

function isoFromEpochSeconds(value) {
  if (!finite(value) || value <= 0) return undefined;
  const date = new Date(value * 1000);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
}

function kindForDuration(mins) {
  return mins >= MONTH_MINS ? "monthly" : mins >= WEEK_MINS ? "weekly" : "session";
}

const labelForKind = (kind) => (kind === "session" ? "Session" : kind === "weekly" ? "Weekly" : "Monthly");

function codexRateLimitsToWindows(snapshot) {
  if (!snapshot || (snapshot.limitId && snapshot.limitId !== "codex")) return [];
  const isMonthlyPlan = snapshot.planType === "free" || snapshot.planType === "go";
  const positions = [
    ["primary", snapshot.primary, isMonthlyPlan ? MONTH_MINS : SESSION_MINS],
    ["secondary", snapshot.secondary, WEEK_MINS],
  ];
  const windows = [];
  for (const [id, window, fallbackMins] of positions) {
    if (!window || !finite(window.usedPercent)) continue;
    const windowDurationMins = finite(window.windowDurationMins) ? window.windowDurationMins : fallbackMins;
    const kind = kindForDuration(windowDurationMins);
    const resetsAt = isoFromEpochSeconds(window.resetsAt);
    windows.push({ id, kind, label: labelForKind(kind), usedPercent: clampPercent(window.usedPercent), windowDurationMins, ...(resetsAt ? { resetsAt } : {}) });
  }
  if (snapshot.individualLimit && finite(snapshot.individualLimit.remainingPercent)) {
    const resetsAt = isoFromEpochSeconds(snapshot.individualLimit.resetsAt);
    windows.push({ id: "individual", kind: "monthly", label: "Monthly credit limit", usedPercent: clampPercent(100 - snapshot.individualLimit.remainingPercent), ...(resetsAt ? { resetsAt } : {}) });
  }
  return windows;
}

/** account/rateLimits/read as the host's `{checkedAt, windows, authLabel?}` limit shape. */
export function codexLimits(input) {
  const checkedAt = typeof input?.checkedAt === "string" ? input.checkedAt : new Date().toISOString();
  const byId = input?.rateLimitsByLimitId;
  const snapshot = byId?.codex ?? input?.rateLimits ?? input?.snapshot ?? input;
  const windows = codexRateLimitsToWindows(snapshot);
  const authLabel = codexPlanLabel(snapshot?.planType);
  return { checkedAt, windows, ...(authLabel ? { authLabel } : {}) };
}

// T3 CodexTurnTokenUsage: thread-wide totals, minus the baseline, for the active turn. A late update
// from a finished turn only moves the baseline.
function usageBreakdown(usage) {
  return {
    inputTokens: usage?.inputTokens ?? 0,
    cachedInputTokens: usage?.cachedInputTokens ?? 0,
    ...(usage?.cacheWriteInputTokens !== undefined ? { cacheCreationTokens: usage.cacheWriteInputTokens } : {}),
    outputTokens: usage?.outputTokens ?? 0,
    reasoningTokens: usage?.reasoningOutputTokens ?? 0,
  };
}

function makeTurnUsageState() {
  return { baseline: undefined, activeTurnId: undefined, byTurnId: new Map() };
}

function beginTurnUsage(state, turnId) {
  state.activeTurnId = turnId;
  if (!state.byTurnId.has(turnId)) {
    state.byTurnId.set(turnId, { inputTokens: 0, cachedInputTokens: 0, cacheCreationTokens: 0, outputTokens: 0, reasoningTokens: 0, observed: false });
  }
}

function usageDelta(previous, current, last) {
  if (previous === undefined || current.inputTokens < previous.inputTokens || current.cachedInputTokens < previous.cachedInputTokens || current.outputTokens < previous.outputTokens || current.reasoningTokens < previous.reasoningTokens) return last;
  return {
    inputTokens: current.inputTokens - previous.inputTokens,
    cachedInputTokens: current.cachedInputTokens - previous.cachedInputTokens,
    ...(current.cacheCreationTokens !== undefined && previous.cacheCreationTokens !== undefined && current.cacheCreationTokens >= previous.cacheCreationTokens ? { cacheCreationTokens: current.cacheCreationTokens - previous.cacheCreationTokens } : {}),
    outputTokens: current.outputTokens - previous.outputTokens,
    reasoningTokens: current.reasoningTokens - previous.reasoningTokens,
  };
}

function accumulateTurnUsage(state, turnId, tokenUsage) {
  const current = usageBreakdown(tokenUsage?.total);
  if (state.activeTurnId !== turnId) {
    state.baseline = current;
    return;
  }
  beginTurnUsage(state, turnId);
  const accumulator = state.byTurnId.get(turnId);
  const delta = usageDelta(state.baseline, current, usageBreakdown(tokenUsage?.last));
  state.baseline = current;
  if (delta.inputTokens > 0 || delta.cachedInputTokens > 0 || delta.outputTokens > 0 || delta.reasoningTokens > 0) accumulator.observed = true;
  accumulator.inputTokens += delta.inputTokens;
  accumulator.cachedInputTokens += delta.cachedInputTokens;
  accumulator.outputTokens += delta.outputTokens;
  accumulator.reasoningTokens += delta.reasoningTokens;
  if (delta.cacheCreationTokens === undefined) accumulator.cacheCreationTokens = undefined;
  else if (accumulator.cacheCreationTokens !== undefined) accumulator.cacheCreationTokens += delta.cacheCreationTokens;
}

function completeTurnUsage(state, turnId) {
  const usage = state.byTurnId.get(turnId);
  state.byTurnId.delete(turnId);
  if (state.activeTurnId === turnId) state.activeTurnId = undefined;
  if (!usage?.observed) return undefined;
  return {
    inputTokens: usage.inputTokens,
    cachedInputTokens: Math.min(usage.inputTokens, usage.cachedInputTokens),
    ...(usage.cacheCreationTokens !== undefined ? { cacheCreationTokens: Math.min(usage.inputTokens, usage.cacheCreationTokens) } : {}),
    outputTokens: usage.outputTokens,
    reasoningTokens: Math.min(usage.outputTokens, usage.reasoningTokens),
  };
}

const mergeUsage = (left, right) => {
  if (!left) return right;
  if (!right) return left;
  return {
    inputTokens: left.inputTokens + right.inputTokens,
    cachedInputTokens: left.cachedInputTokens + right.cachedInputTokens,
    ...(left.cacheCreationTokens !== undefined && right.cacheCreationTokens !== undefined ? { cacheCreationTokens: left.cacheCreationTokens + right.cacheCreationTokens } : {}),
    outputTokens: left.outputTokens + right.outputTokens,
    reasoningTokens: left.reasoningTokens + right.reasoningTokens,
  };
};

class CodexAppServerClient {
  constructor({ child, spawnError, onNotification, onRequest, onExit, onProtocolError }) {
    this.child = child;
    this.onNotification = onNotification;
    this.onRequest = onRequest;
    this.onExit = onExit;
    this.onProtocolError = onProtocolError;
    this.pending = new Map();
    this.nextId = 1;
    this.closed = false;
    this.exited = false;
    this.stderrTail = [];
    this.lines = createInterface({ input: child.stdout });
    this.lines.on("line", (line) => this.onLine(line));
    child.stdout.on("error", () => {});
    child.stderr?.on("data", (chunk) => {
      this.stderrTail.push(String(chunk));
      if (this.stderrTail.length > 20) this.stderrTail.shift();
      process.stderr.write(chunk);
    });
    child.on("error", (cause) => {
      if (this.exited) return;
      const error =
        cause?.code === "ENOENT"
          ? hostError("CodexNotFound", "Codex is not installed or not on PATH.", "engine", spawnError ?? cause)
          : hostError("CodexTransportError", "Codex could not be started.", "network", spawnError ?? cause);
      this.failAll(error);
      this.terminate(-1, null, cause);
    });
    child.on("exit", (code, signal) => this.terminate(code, signal));
  }

  onLine(line) {
    if (line.trim() === "") return;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      this.onProtocolError?.("Codex wrote a line that is not JSON.");
      return;
    }
    if (typeof message !== "object" || message === null) return;
    if (message.id !== undefined && message.method === undefined) return this.onResponse(message);
    if (message.id !== undefined && typeof message.method === "string") return void this.onServerRequest(message);
    if (typeof message.method === "string") return this.onNotification?.(message);
  }

  onResponse(message) {
    const key = String(message.id);
    const pending = this.pending.get(key);
    if (!pending) return;
    this.pending.delete(key);
    clearTimeout(pending.timer);
    if (message.error !== undefined) {
      pending.reject(Object.assign(new Error(String(message.error?.message ?? "Codex request failed")), { name: "CodexRequestError", code: message.error?.code }));
    } else {
      pending.resolve(message.result);
    }
  }

  async onServerRequest(message) {
    if (!this.onRequest) {
      this.respondError(message.id, -32601, "Method not found");
      return;
    }
    try {
      const result = await this.onRequest(message);
      if (result !== undefined) this.respond(message.id, result);
    } catch {
      this.respondError(message.id, -32603, "Codex request failed");
    }
  }

  failAll(error) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
  }

  terminate(code, signal, cause) {
    if (this.exited) return;
    this.exited = true;
    this.closed = true;
    const error = hostError("CodexProcessExitedError", code === undefined || code < 0 ? "Codex exited before it answered." : `Codex exited (${code}).`, "engine", cause);
    error.code = code;
    this.failAll(error);
    this.lines.close();
    this.child.stdout.destroy?.();
    this.onExit?.({ code, signal, stderr: this.stderrTail.join("") });
  }

  write(frame) {
    if (this.closed) throw hostError("CodexClosed", "Codex is no longer running.", "engine");
    try {
      this.child.stdin.write(`${JSON.stringify(frame)}\n`);
    } catch (cause) {
      throw hostError("CodexClosed", "Codex is no longer running.", "engine", cause);
    }
  }

  request(method, params, timeoutMs = REQUEST_TIMEOUT_MS) {
    if (this.closed) return Promise.reject(hostError("CodexClosed", "Codex is no longer running.", "engine"));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(String(id));
        reject(hostError("CodexTimeout", `Codex ${method} timed out.`, "network"));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(String(id), { resolve, reject, timer, method });
      try {
        this.write({ id, method, ...(params !== undefined ? { params } : {}) });
      } catch (cause) {
        this.pending.delete(String(id));
        clearTimeout(timer);
        reject(cause);
      }
    });
  }

  notify(method, params) {
    this.write({ method, ...(params !== undefined ? { params } : {}) });
  }

  respond(id, result) {
    // A reply to a dead process has nowhere to go; the pending ask is already refused locally.
    try {
      this.write({ id, result });
    } catch {}
  }

  respondError(id, code, message) {
    try {
      this.write({ id, error: { code, message } });
    } catch {}
  }

  async close() {
    if (this.exited) return;
    this.closed = true;
    this.failAll(hostError("CodexClosed", "Codex is no longer running.", "engine"));
    try {
      this.child.stdin.end();
    } catch {}
    try {
      this.child.kill();
    } catch {}
  }
}

const cancellationResponse = (entry, mode) => {
  switch (entry.method) {
    case "item/commandExecution/requestApproval":
    case "item/fileChange/requestApproval":
      return { decision: mode === "cancel" ? "cancel" : "decline" };
    case "item/permissions/requestApproval":
      return { permissions: {}, scope: "turn" };
    case "applyPatchApproval":
    case "execCommandApproval":
      return { decision: mode === "cancel" ? "abort" : { denied: { rejection: "User declined the request." } } };
    case "mcpServer/elicitation/request":
      return { action: mode === "cancel" ? "cancel" : "decline" };
    case "item/tool/requestUserInput":
      return { answers: Object.fromEntries((entry.params?.questions ?? []).map((question) => [question.id, { answers: [] }])) };
    default:
      return {};
  }
};

export function codexFamily({ spawn = defaultSpawn, startTimeoutMs = START_TIMEOUT_MS, requestTimeoutMs = REQUEST_TIMEOUT_MS, wakeGraceMs = WAKE_GRACE_MS, holdLimitMs = HOLD_LIMIT_MS, quietMs = 5 * 60_000 } = {}) {
  return {
    name: "codex",
    sdk: { name: "codex app-server", version: SDK_VERSION },
    capabilities: ["steer", "fork", "rollback", "models", "usage", "readHistory"],
    create(ctx) {
      const sessions = new Map(); // host sessionId -> session
      const processes = new Map(); // confinement key -> process record
      const live = new Set(); // every app-server this host started, including throwaways
      const turnHistory = new Map(); // engineSessionId -> turn ids this host has seen, across resumes
      const pending = new Map(); // requestId -> {session, method, params, resolve, responded}
      let counter = 0;

      const emit = (run, frame) => (run?.open ? ctx.send(frame) : run?.buffer.push(frame));
      const sendIdOf = (session) => (session.run ? { sendId: session.run.sendId } : {});
      const sessionOf = (id) => {
        const session = sessions.get(id);
        if (!session) throw hostError("UnknownSession", "unknown session", "invalid_request");
        return session;
      };
      const usableProcess = (session) => Boolean(session.process && !session.process.exited && !session.process.client.closed);
      const runOf = (sendId) => {
        for (const session of sessions.values()) if (session.run?.sendId === sendId) return session;
        return undefined;
      };

      const mapThread = (proc, threadId, session) => {
        if (typeof threadId === "string" && threadId !== "") proc.threadToSession.set(threadId, session);
      };

      const sessionForThread = (proc, threadId) => {
        if (typeof threadId === "string" && proc.threadToSession.has(threadId)) return proc.threadToSession.get(threadId);
        for (const session of sessions.values()) if (session.process === proc && session.engineSessionId === threadId) return session;
        const owners = [...sessions.values()].filter((session) => session.process === proc && !session.closing);
        return owners.length === 1 ? owners[0] : undefined;
      };

      const rememberTurn = (session, turnId) => {
        if (typeof turnId !== "string" || turnId === "") return;
        if (!session.turns.includes(turnId)) session.turns.push(turnId);
        turnHistory.set(session.engineSessionId, [...session.turns]);
      };

      const background = (session, event) => {
        const frame = { type: "background", sessionId: session.id, event };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };

      const agentsRunning = (session) => [...session.agents.values()].some((agent) => agent.running);

      const startAgent = (session, id, extra = {}) => {
        if (typeof id !== "string" || id === "") return;
        const known = session.agents.get(id);
        if (known?.running) return;
        session.agents.set(id, { running: true });
        background(session, { type: "started", id, kind: "agent", ...(extra.toolCallId ? { toolCallId: extra.toolCallId } : {}), ...(extra.description ? { description: extra.description } : {}) });
      };

      const endAgent = (session, id, status) => {
        const known = session.agents.get(id);
        if (!known?.running) return;
        session.agents.delete(id);
        background(session, { type: "ended", id, kind: "agent", status: status ?? "completed" });
        onAgentsSettled(session);
      };

      const stopAgents = (session, status) => {
        for (const id of [...session.agents.keys()]) {
          session.agents.delete(id);
          background(session, { type: "ended", id, kind: "agent", status });
        }
      };

      const finish = (session, run, frame) => {
        if (!run || run.done) return;
        run.done = true;
        clearTimeout(run.grace);
        clearTimeout(run.holdLimit);
        clearTimeout(run.quiet);
        if (session.run === run) session.run = null;
        if (frame.result?.usage) session.lastUsage = frame.result.usage;
        for (const [id, entry] of pending) {
          if (entry.session !== session) continue;
          pending.delete(id);
          entry.resolve(cancellationResponse(entry, "decline"));
        }
        emit(run, { type: "runDone", sendId: run.sendId, ...frame });
        run.ended?.();
      };

      const failure = (error, { usage, threadDisposition = "reusable" } = {}) => {
        const retry = retryFor(error);
        return {
          ok: false,
          result: {
            reason: error.reason ?? "failed",
            threadDisposition,
            ...(retry ? { retry } : {}),
            ...(usage ? { usage } : {}),
          },
          error: {
            method: "send",
            cause: {
              name: error.name ?? "Error",
              message: error.message ?? String(error),
              ...(error.kind ? { kind: error.kind } : {}),
              ...(typeof error.code === "number" ? { code: error.code } : {}),
            },
          },
        };
      };

      const turnResult = (session, run, turn, usage) => {
        const status = turn?.status;
        if (status === "completed") return { ok: true, result: { reason: "completed", threadDisposition: "reusable", ...(usage ? { usage } : {}) } };
        if (status === "interrupted") return { ok: true, result: { reason: "cancelled", threadDisposition: "reusable", ...(usage ? { usage } : {}) } };
        const error = codexTurnError(turn?.error);
        return failure(error, { usage, ...(error.kind === "session_lost" ? { threadDisposition: "broken" } : {}) });
      };

      const cancelledFrame = (run) => {
        const usage = run.turnId === undefined ? undefined : completeTurnUsage(run.usageState, run.turnId);
        return { ok: true, result: { reason: "cancelled", threadDisposition: "reusable", ...(usage ? { usage } : {}) } };
      };

      const onAgentsSettled = (session) => {
        const run = session.run;
        if (!run?.held || agentsRunning(session)) return;
        clearTimeout(run.grace);
        run.grace = setTimeout(() => {
          if (session.run !== run || !run.held) return;
          finish(session, run, run.heldFrame ?? { ok: true, result: { reason: "completed", threadDisposition: "reusable" } });
        }, wakeGraceMs);
      };

      const settleTurn = (session, run, turn) => {
        let usage = completeTurnUsage(run.usageState, turn.id);
        if (run.heldUsage) {
          usage = mergeUsage(run.heldUsage, usage);
          run.heldUsage = undefined;
        }
        const frame = turnResult(session, run, turn, usage);
        if (agentsRunning(session) && frame.result.reason !== "cancelled") {
          if (!run.held) ctx.log({ type: "runHeld", why: "agent" });
          run.held = true;
          run.heldFrame = frame;
          run.heldUsage = usage;
          clearTimeout(run.quiet);
          run.quiet = setTimeout(() => {
            if (session.run !== run || !run.held) return;
            ctx.log({ type: "runQuiet", minutes: Math.round(quietMs / 60_000) });
            stopAgents(session, "stopped");
            finish(session, run, frame);
          }, quietMs);
          run.quiet.unref?.();
          clearTimeout(run.holdLimit);
          run.holdLimit = setTimeout(() => {
            if (session.run !== run || !run.held) return;
            ctx.log({ type: "runHoldLimit", minutes: Math.round(holdLimitMs / 60_000) });
            stopAgents(session, "stopped");
            finish(session, run, frame);
          }, holdLimitMs);
          run.holdLimit.unref?.();
          return;
        }
        finish(session, run, frame);
      };

      const emitDelta = (session, notification) => {
        const run = session.run;
        if (!run) {
          ctx.log({ type: "engineNotification", method: notification.method });
          return;
        }
        emit(run, { type: "delta", sendId: run.sendId, update: { method: notification.method, params: notification.params } });
      };

      const toolName = (method, params = {}) => {
        switch (method) {
          case "item/commandExecution/requestApproval":
            return "command";
          case "item/fileChange/requestApproval":
            return "fileChange";
          case "item/permissions/requestApproval":
            return "permissions";
          case "item/tool/requestUserInput":
            return "request_user_input";
          case "mcpServer/elicitation/request":
            return typeof params.serverName === "string" && params.serverName !== "" ? params.serverName : "mcpServer";
          case "applyPatchApproval":
            return "applyPatch";
          case "execCommandApproval":
            return "execCommand";
          default:
            return method;
        }
      };

      const publish = (session, entry) => {
        const kind = entry.question ? "question" : "permission";
        const frame = {
          type: kind,
          ...sendIdOf(session),
          requestId: entry.requestId,
          tool: toolName(entry.method, entry.params),
          input: entry.params,
          ...(entry.question ? {} : { options: entry.options }),
        };
        if (session.run) emit(session.run, frame);
        else ctx.send(frame);
      };

      const ask = (session, message, definition) => {
        const requestId = `${definition.question ? "q" : "p"}-${++counter}`;
        return new Promise((resolve) => {
          const entry = {
            session,
            method: message.method,
            params: message.params ?? {},
            engineRequestId: message.id,
            requestId,
            question: definition.question === true,
            options: definition.options ?? [],
            resolve,
          };
          pending.set(requestId, entry);
          publish(session, entry);
        });
      };

      const handleServerRequest = async (proc, message) => {
        const method = message.method;
        const params = message.params ?? {};
        const session = sessionForThread(proc, params.threadId ?? params.conversationId);
        if (!session) return cancellationResponse({ method, params, question: method === "item/tool/requestUserInput" }, "cancel");
        if (!APPROVAL_METHODS.has(method) && method !== "item/tool/requestUserInput") {
          throw hostError("UnsupportedServerRequest", `Codex sent an unsupported request: ${method}`, "unsupported");
        }
        const options =
          method === "item/commandExecution/requestApproval" && Array.isArray(params.availableDecisions) && params.availableDecisions.length > 0
            ? params.availableDecisions
            : method === "item/fileChange/requestApproval"
              ? ["accept", "decline", "cancel"]
              : method === "item/permissions/requestApproval"
                ? ["accept", "decline"]
                : method === "mcpServer/elicitation/request"
                  ? ["accept", "decline", "cancel"]
                  : ["accept", "decline"];
        const question = method === "item/tool/requestUserInput" || (method === "mcpServer/elicitation/request" && params.mode === "form" && params.requestedSchema?.properties);
        return ask(session, message, { question, options });
      };

      const handleRootTurnCompleted = (session, notification) => {
        const turn = notification.params?.turn;
        const run = session.run;
        if (!run || run.done) return;
        if (run.held) {
          if (turn.id === run.turnId) return; // a duplicate of the turn that put the run on hold
          // The follow-up turn the agents' result starts is the prompt's answer; adopt it even when
          // its turn/started was missed.
          run.held = false;
          run.heldFrame = undefined;
          clearTimeout(run.grace);
          clearTimeout(run.quiet);
          run.turnId = turn.id;
          beginTurnUsage(run.usageState, turn.id);
          rememberTurn(session, turn.id);
          emit(run, { type: "session", sendId: run.sendId, engineSessionId: session.engineSessionId, turnId: turn.id });
          settleTurn(session, run, turn);
          return;
        }
        // A completed turn with no id ends the run, whatever turn was expected.
        if (turn?.id !== undefined) {
          if (run.turnId === undefined) {
            run.turnId = turn.id;
            beginTurnUsage(run.usageState, turn.id);
            rememberTurn(session, turn.id);
          } else if (run.turnId !== turn.id) {
            return; // a leftover turn the engine started for itself: shown, never the end
          }
        }
        settleTurn(session, run, turn);
      };

      const resetQuiet = (session, run) => {
        clearTimeout(run.quiet);
        run.quiet = setTimeout(() => {
          if (session.run !== run || !run.held) return;
          ctx.log({ type: "runQuiet", minutes: Math.round(quietMs / 60_000) });
          stopAgents(session, "stopped");
          finish(session, run, run.heldFrame);
        }, quietMs);
        run.quiet.unref?.();
      };

      const handleNotification = (proc, notification) => {
        const { method, params = {} } = notification;
        const threadId = params.threadId ?? params.thread?.id;
        // Any notification while a run is held restarts the quiet window.
        const heldSessions = threadId === undefined
          ? [...sessions.values()].filter((candidate) => candidate.process === proc && candidate.run?.held)
          : [sessionForThread(proc, threadId)].filter((candidate) => candidate?.run?.held);
        for (const heldSession of heldSessions) resetQuiet(heldSession, heldSession.run);
        if (method === "serverRequest/resolved") {
          // Codex settled the request itself (its turn ended); the handler must not wait or reply.
          for (const [id, entry] of pending) {
            if (String(entry.engineRequestId) !== String(params.requestId)) continue;
            pending.delete(id);
            entry.resolve(undefined);
          }
        }
        if (method === "thread/started" && params.thread?.parentThreadId) {
          const parent = sessionForThread(proc, params.thread.parentThreadId);
          if (parent) {
            mapThread(proc, params.thread.id, parent);
            startAgent(parent, params.thread.id, { description: params.thread.agentRole ?? undefined });
            emitDelta(parent, notification);
          }
          return;
        }
        let session = sessionForThread(proc, threadId);
        if (!session) {
          // Account and warning notifications carry no thread: show them on every run in the process.
          const owners = [...sessions.values()].filter((candidate) => candidate.process === proc && candidate.run && !candidate.closing);
          if (owners.length > 0) {
            for (const owner of owners) emitDelta(owner, notification);
          } else {
            ctx.log({ type: "engineNotification", method });
          }
          return;
        }
        if (method === "thread/started") {
          const thread = params.thread ?? {};
          if (thread.parentThreadId) {
            const parent = sessionForThread(proc, thread.parentThreadId);
            if (parent) {
              mapThread(proc, thread.id, parent);
              startAgent(parent, thread.id, { description: thread.agentRole ?? undefined });
              session = parent;
            }
          } else if (session.engineSessionId === undefined) {
            session.engineSessionId = thread.id;
            mapThread(proc, thread.id, session);
            if (session.run) emit(session.run, { type: "session", sendId: session.run.sendId, engineSessionId: thread.id });
          } else {
            mapThread(proc, thread.id, session);
          }
        } else if (method === "thread/status/changed") {
          if (threadId !== session.engineSessionId) {
            mapThread(proc, threadId, session);
            const status = params.status?.type;
            if (status === "active") startAgent(session, threadId);
            else if (status === "idle") endAgent(session, threadId, "completed");
            else if (status === "systemError") endAgent(session, threadId, "failed");
          }
        } else if (method === "thread/closed") {
          emitDelta(session, notification);
          if (threadId === session.engineSessionId) {
            lose(session, hostError("CodexThreadClosed", "Codex closed this thread.", "session_lost"));
          } else {
            endAgent(session, threadId, "closed");
          }
          return;
        } else if (method === "turn/started") {
          const turn = params.turn ?? {};
          if (threadId === session.engineSessionId && session.run) {
            const run = session.run;
            let own = false;
            if (run.turnId === undefined) {
              run.turnId = turn.id;
              own = true;
            } else if (run.held && turn.id !== run.turnId) {
              run.held = false;
              run.heldFrame = undefined;
              clearTimeout(run.grace);
              clearTimeout(run.quiet);
              run.turnId = turn.id;
              own = true;
            } else if (run.turnId === turn.id) {
              own = true;
            }
            if (own) {
              beginTurnUsage(run.usageState, turn.id);
              rememberTurn(session, turn.id);
              emit(run, { type: "session", sendId: run.sendId, engineSessionId: session.engineSessionId, turnId: run.turnId });
            }
          }
        } else if (method === "turn/completed") {
          emitDelta(session, notification);
          if (threadId === session.engineSessionId) handleRootTurnCompleted(session, notification);
          return;
        } else if (method === "thread/tokenUsage/updated") {
          const run = session.run;
          if (run && (run.turnId === undefined || run.turnId === params.turnId || params.turnId === undefined)) {
            accumulateTurnUsage(run.usageState, params.turnId ?? run.turnId, params.tokenUsage);
          }
        } else if (method === "item/started" || method === "item/completed") {
          const item = params.item ?? {};
          if (item.type === "subAgentActivity") {
            mapThread(proc, item.agentThreadId, session);
            if (item.kind === "started") startAgent(session, item.agentThreadId, { toolCallId: item.id, description: item.agentPath });
            else endAgent(session, item.agentThreadId, item.kind === "completed" ? "completed" : "failed");
          } else if (item.type === "collabAgentToolCall") {
            for (const id of item.receiverThreadIds ?? []) mapThread(proc, id, session);
            for (const [id, agent] of Object.entries(item.agentsStates ?? {})) {
              mapThread(proc, id, session);
              const status = agent?.status ?? "";
              if (/complete|error|shutdown|failed/i.test(status)) endAgent(session, id, /error|fail/i.test(status) ? "failed" : "completed");
              else startAgent(session, id, { toolCallId: item.id });
            }
          }
        } else if (method === "error") {
          emitDelta(session, notification);
          const run = session.run;
          if (run && !params.willRetry && (run.turnId === undefined || run.turnId === params.turnId)) {
            const usage = run.turnId === undefined ? undefined : completeTurnUsage(run.usageState, run.turnId);
            const error = codexTurnError(params.error);
            finish(session, run, failure(error, { usage, ...(error.kind === "session_lost" ? { threadDisposition: "broken" } : {}) }));
          }
          return;
        }
        emitDelta(session, notification);
      };

      const handleProcessExit = (proc, exit) => {
        proc.exited = true;
        live.delete(proc);
        if (processes.get(proc.key) === proc) processes.delete(proc.key);
        if (proc.closing) return;
        for (const session of [...sessions.values()]) {
          if (session.process !== proc) continue;
          for (const [id, entry] of pending) {
            if (entry.session !== session) continue;
            pending.delete(id);
            entry.resolve(undefined);
          }
          if (session.run) {
            session.broken = true;
            stopAgents(session, "stopped");
            const error = hostError("CodexStopped", "Codex stopped. Your work in the project is kept; send your message again.", "engine");
            finish(session, session.run, failure(error, { threadDisposition: "broken" }));
          }
        }
      };

      const lose = (session, cause) => {
        if (session.closing || session.broken) return;
        session.broken = true;
        const error = mapCodexError(cause, "session_lost");
        ctx.log({ type: "codexLost", kind: error.kind });
        if (session.run) {
          stopAgents(session, "stopped");
          finish(session, session.run, failure(error, { threadDisposition: "broken" }));
        }
        for (const [id, entry] of pending) {
          if (entry.session !== session) continue;
          pending.delete(id);
          entry.resolve(undefined);
        }
      };

      const confinementKey = (request, options) => {
        const confinement = request.confinement;
        return JSON.stringify({
          prefix: confinement?.argvPrefix ?? null,
          env: confinement?.env ?? null,
          cwd: confinement?.cwd ?? null,
          codexPath: options.codexPath ?? null,
          added: options.env ?? options.envAdditions ?? null,
        });
      };

      const startProcess = async (request, options, { reuse = true } = {}) => {
        const confinement = request.confinement;
        if (confinement !== undefined && (typeof confinement !== "object" || confinement === null || Array.isArray(confinement))) {
          throw hostError("InvalidConfinement", "confinement must be an object.", "invalid_request");
        }
        if (confinement?.env !== undefined && (typeof confinement.env !== "object" || confinement.env === null || Array.isArray(confinement.env))) {
          throw hostError("InvalidConfinement", "confinement.env must be an object.", "invalid_request");
        }
        if (confinement?.cwd !== undefined && typeof confinement.cwd !== "string") {
          throw hostError("InvalidConfinement", "confinement.cwd must be a path.", "invalid_request");
        }
        const prefix = confinement?.argvPrefix;
        if (prefix !== undefined && !(Array.isArray(prefix) && prefix.length > 0 && prefix.every((part) => typeof part === "string" && part.length > 0))) {
          throw hostError("InvalidConfinement", "confinement.argvPrefix must be a list of non-empty strings.", "invalid_request");
        }
        const key = confinementKey(request, options);
        const existing = reuse ? processes.get(key) : undefined;
        if (existing && !existing.exited && !existing.closing) return existing;
        const fromCore = confinement?.env !== undefined;
        const env = {};
        for (const [name, value] of Object.entries(fromCore ? confinement.env : process.env)) {
          if (typeof value === "string") env[name] = value;
        }
        const additions = options.env ?? options.envAdditions;
        const added = [];
        if (additions !== undefined) {
          if (typeof additions !== "object" || additions === null || Array.isArray(additions)) throw hostError("InvalidEnv", "options.env must be an object of strings.", "invalid_request");
          for (const [name, value] of Object.entries(additions)) {
            if (typeof value !== "string") throw hostError("InvalidEnv", "options.env values must be strings.", "invalid_request");
            if (env[name] !== value) added.push(name);
            env[name] = value;
          }
        }
        if (added.length > 0) ctx.log({ type: "envAdded", keys: added });
        const binary = options.codexPath ?? "codex";
        const launch = prefix === undefined ? codexLaunch(binary, env) : { command: binary, args: [] };
        const [command, ...rest] = prefix === undefined ? [launch.command, ...launch.args] : [...prefix, binary];
        const args = [...rest, ...APP_SERVER_ARGS];
        let child;
        try {
          child = spawn({ command, args, cwd: confinement?.cwd ?? request.cwd ?? process.cwd(), env });
        } catch (cause) {
          throw mapCodexError(cause);
        }
        let proc;
        const client = new CodexAppServerClient({
          child,
          onNotification: (notification) => handleNotification(proc, notification),
          onRequest: (message) => handleServerRequest(proc, message),
          onExit: (exit) => handleProcessExit(proc, exit),
          onProtocolError: (message) => ctx.log({ type: "protocolError", message }),
        });
        proc = { key, child, client, sessions: new Set(), threadToSession: new Map(), exited: false, closing: false, throwaway: !reuse };
        live.add(proc);
        if (reuse) processes.set(key, proc);
        try {
          await client.request(
            "initialize",
            { clientInfo: { name: CLIENT_NAME, title: CLIENT_NAME, version: "0.1.0" }, capabilities: { experimentalApi: true } },
            startTimeoutMs,
          );
          client.notify("initialized");
          return proc;
        } catch (cause) {
          proc.closing = true;
          live.delete(proc);
          if (processes.get(key) === proc) processes.delete(key);
          await client.close();
          throw mapCodexError(cause);
        }
      };

      const threadConfig = (options) => {
        const config = {};
        if (options.mcpServers !== undefined) {
          if (typeof options.mcpServers !== "object" || options.mcpServers === null || Array.isArray(options.mcpServers)) {
            throw hostError("InvalidMcpServers", "options.mcpServers must be an object.", "invalid_request");
          }
          for (const [name, server] of Object.entries(options.mcpServers)) config[`mcp_servers.${name}`] = server;
        }
        return Object.keys(config).length > 0 ? config : undefined;
      };

      const threadParams = (session) => {
        const options = session.options;
        const config = threadConfig(options);
        return {
          cwd: session.cwd,
          ...(typeof options.model === "string" ? { model: options.model } : {}),
          ...(typeof options.developerInstructions === "string" ? { developerInstructions: options.developerInstructions } : {}),
          ...(config ? { config } : {}),
        };
      };

      const turnParams = (session) => {
        const options = session.options;
        const effort = session.effort;
        return {
          threadId: session.engineSessionId,
          cwd: session.cwd,
          ...(typeof options.model === "string" ? { model: options.model } : {}),
          ...(typeof effort === "string" ? { effort } : {}),
          approvalPolicy: session.approvalPolicy,
          approvalsReviewer: "user",
          sandboxPolicy: options.mode === WORKSPACE_WRITE ? { type: "workspaceWrite" } : { type: "readOnly" },
          summary: "auto",
        };
      };

      const messageInput = (message) => {
        const text = typeof message === "string" ? message : message?.text;
        const input = [];
        if (typeof text === "string" && text !== "") input.push({ type: "text", text });
        const attachments = typeof message === "object" ? (message.attachments ?? []) : [];
        if (!Array.isArray(attachments)) throw hostError("InvalidMessage", "message.attachments must be a list.", "invalid_request");
        for (const attachment of attachments) {
          const path = typeof attachment === "string" ? attachment : attachment?.path;
          if (typeof path !== "string" || path === "") throw hostError("InvalidAttachment", "an attachment needs a path.", "invalid_request");
          const mime = typeof attachment === "object" ? attachment.mimeType : undefined;
          input.push({ type: typeof mime === "string" && mime.startsWith("audio/") ? "localAudio" : "localImage", path });
        }
        if (input.length === 0) throw hostError("InvalidMessage", "message must be text or attachments by path.", "invalid_request");
        return input;
      };

      const answerFor = (entry, decision) => {
        if (entry.method === "item/tool/requestUserInput") {
          const answers = {};
          for (const question of entry.params.questions ?? []) {
            const raw = decision.answers?.[question.question] ?? decision.answers?.[question.id];
            const values = Array.isArray(raw) ? raw.map(String) : typeof raw === "string" ? raw.split(",").map((part) => part.trim()).filter(Boolean) : [];
            answers[question.id] = { answers: values };
          }
          return { answers };
        }
        if (entry.method === "mcpServer/elicitation/request") {
          if (entry.question) {
            if (decision.behavior === "deny") return { action: "decline" };
            const properties = entry.params.requestedSchema?.properties ?? {};
            const content = {};
            for (const [key, value] of Object.entries(decision.answers ?? {})) {
              const match = Object.hasOwn(properties, key) ? key : Object.keys(properties).find((candidate) => properties[candidate]?.title === key);
              if (match) content[match] = value;
            }
            if (entry.params.requestedSchema?.required?.some((key) => !Object.hasOwn(content, key))) return { action: "decline" };
            return { action: "accept", ...(Object.keys(content).length > 0 ? { content } : {}) };
          }
          if (decision.behavior !== "allow") return { action: "decline" };
          // T3 cannot act on a URL-mode elicitation; accepting would claim it opened the page.
          if (entry.params.mode === "url") {
            ctx.log({ type: "elicitationUrlUnsupported" });
            return { action: "cancel" };
          }
          return { action: "accept" };
        }
        if (decision.behavior === "allow") {
          switch (entry.method) {
            case "item/commandExecution/requestApproval":
            case "item/fileChange/requestApproval":
              return { decision: "accept" };
            case "item/permissions/requestApproval":
              return { permissions: entry.params.permissions ?? {}, scope: "turn" };
            case "applyPatchApproval":
            case "execCommandApproval":
              return { decision: "approved" };
            default:
              return { decision: "accept" };
          }
        }
        const message = typeof decision.message === "string" ? decision.message : "User declined the request.";
        switch (entry.method) {
          case "item/commandExecution/requestApproval":
          case "item/fileChange/requestApproval":
            return { decision: "decline" };
          case "item/permissions/requestApproval":
            return { permissions: {}, scope: "turn" };
          case "applyPatchApproval":
          case "execCommandApproval":
            return { decision: { denied: { rejection: message } } };
          default:
            return { decision: "decline" };
        }
      };

      const refusePending = (session, mode) => {
        for (const [id, entry] of pending) {
          if (entry.session !== session) continue;
          pending.delete(id);
          entry.resolve(cancellationResponse(entry, mode));
        }
      };

      const closeProcess = async (proc) => {
        if (proc.exited || proc.closing) return;
        proc.closing = true;
        live.delete(proc);
        if (processes.get(proc.key) === proc) processes.delete(proc.key);
        await proc.client.close();
      };

      const withThrowaway = async (request, options, run) => {
        const proc = await startProcess({ cwd: request.cwd ?? process.cwd(), confinement: request.confinement }, options, { reuse: false });
        try {
          return await run(proc);
        } finally {
          await closeProcess(proc);
        }
      };

      return {
        async handle(request) {
          switch (request.type) {
            case "open": {
              const { sessionId, engineSessionId, operation, cwd, options = {} } = request;
              if (typeof sessionId !== "string" || sessionId === "") throw hostError("InvalidOpen", "open needs a sessionId.", "invalid_request");
              if (sessions.has(sessionId)) throw hostError("SessionOpen", "this session is already open", "invalid_request");
              if (typeof operation !== "string") throw hostError("InvalidOpen", "open needs an operation.", "invalid_request");
              if (!["create", "resume", "fork"].includes(operation)) throw hostError("UnsupportedOpen", `Codex cannot ${operation} a session.`, "unsupported");
              if (operation !== "create" && typeof engineSessionId !== "string") throw hostError("InvalidOpen", `${operation} needs engineSessionId.`, "invalid_request");
              if (typeof options !== "object" || options === null || Array.isArray(options)) throw hostError("InvalidOptions", "options must be an object.", "invalid_request");
              const mode = options.mode ?? READ_ONLY;
              if (mode !== READ_ONLY && mode !== WORKSPACE_WRITE) throw hostError("InvalidMode", `options.mode must be "${READ_ONLY}" or "${WORKSPACE_WRITE}".`, "invalid_request");
              const approvalPolicy = options.approvalPolicy ?? ASK_POLICY;
              if (approvalPolicy !== "on-request" && approvalPolicy !== "untrusted") throw hostError("InvalidApprovalPolicy", "options.approvalPolicy may only ask.", "invalid_request");
              const session = {
                id: sessionId,
                engineSessionId,
                cwd: cwd ?? process.cwd(),
                options: { ...options, mode },
                approvalPolicy,
                effort: typeof options.effort === "string" ? options.effort : undefined,
                process: undefined,
                run: null,
                turns: [],
                agents: new Map(),
                lastUsage: null,
                closing: false,
                broken: false,
              };
              const proc = await startProcess(request, options);
              const client = proc.client;
              try {
                let result;
                if (operation === "create") {
                  result = await client.request("thread/start", threadParams(session), requestTimeoutMs);
                } else if (operation === "resume") {
                  session.engineSessionId = engineSessionId;
                  result = await client.request("thread/resume", { threadId: engineSessionId, excludeTurns: true, ...threadParams(session) }, requestTimeoutMs);
                } else {
                  session.engineSessionId = engineSessionId;
                  result = await client.request("thread/fork", { threadId: engineSessionId, excludeTurns: true, ...(typeof options.atTurn === "string" ? { lastTurnId: options.atTurn } : {}), ...threadParams(session) }, requestTimeoutMs);
                }
                const threadId = result?.thread?.id;
                if (typeof threadId !== "string" || threadId === "") throw hostError("CodexThread", "Codex returned no thread id.", "engine");
                session.engineSessionId = threadId;
                session.turns = [...(turnHistory.get(threadId) ?? [])];
                session.process = proc;
                proc.sessions.add(session);
                sessions.set(sessionId, session);
                mapThread(proc, threadId, session);
                return { engineSessionId: threadId };
              } catch (cause) {
                session.closing = true;
                if (proc.sessions.size === 0) await closeProcess(proc);
                throw mapCodexError(cause);
              }
            }
            case "send": {
              const session = sessionOf(request.sessionId);
              if (session.broken) throw hostError("CodexStopped", "Codex stopped. Open the session again.", "session_lost");
              if (session.run) throw hostError("RunInProgress", "a run is already open on this session", "invalid_request");
              if (!usableProcess(session)) throw hostError("CodexStopped", "Codex stopped. Open the session again.", "session_lost");
              const input = messageInput(request.message);
              const effort = typeof request.options?.effort === "string" ? request.options.effort : session.effort;
              const run = {
                sendId: request.id,
                open: false,
                buffer: [],
                done: false,
                turnId: undefined,
                usageState: makeTurnUsageState(),
                held: false,
                heldFrame: undefined,
                heldUsage: undefined,
                grace: undefined,
                holdLimit: undefined,
                messageId: request.messageId,
              };
              session.run = run;
              let accepted;
              try {
                accepted = await session.process.client.request(
                  "turn/start",
                  { ...turnParams(session), ...(typeof effort === "string" ? { effort } : {}), input },
                  requestTimeoutMs,
                );
              } catch (cause) {
                if (session.run === run) session.run = null;
                const error = mapCodexError(cause);
                if (error.kind === "session_lost") session.broken = true;
                throw error;
              }
              const turnId = accepted?.turn?.id;
              if (typeof turnId === "string" && turnId !== "") {
                run.turnId = turnId;
                beginTurnUsage(run.usageState, turnId);
                rememberTurn(session, turnId);
              }
              setImmediate(() => {
                run.open = true;
                for (const frame of run.buffer.splice(0)) ctx.send(frame);
              });
              return {
                runId: `run-${request.id}`,
                engineSessionId: session.engineSessionId,
                ...(typeof run.turnId === "string" ? { turnId: run.turnId } : {}),
                ...(typeof request.messageId === "string" ? { messageId: request.messageId } : {}),
              };
            }
            case "steer": {
              const session = runOf(request.sendId);
              if (!session || session.broken || session.run?.held) return { outcome: "rejected", messageId: request.messageId };
              if (session.run?.turnId === undefined) return { outcome: "rejected", messageId: request.messageId };
              if (typeof request.text !== "string" || request.text === "") throw hostError("InvalidSteer", "steer needs text.", "invalid_request");
              try {
                await session.process.client.request(
                  "turn/steer",
                  { threadId: session.engineSessionId, expectedTurnId: session.run.turnId, input: [{ type: "text", text: request.text }] },
                  requestTimeoutMs,
                );
                return { outcome: "accepted", messageId: request.messageId };
              } catch (cause) {
                ctx.log({ type: "steerRejected", kind: mapCodexError(cause).kind });
                return { outcome: "rejected", messageId: request.messageId };
              }
            }
            case "cancel": {
              const session = runOf(request.sendId);
              if (!session || !session.run) return { cancelled: false };
              const run = session.run;
              refusePending(session, "cancel");
              // Let the client write the refusals before the interrupt, so a reply never follows it.
              await new Promise((resolve) => setImmediate(resolve));
              if (run.held) {
                stopAgents(session, "stopped");
                finish(session, run, cancelledFrame(run));
                return { cancelled: true };
              }
              if (typeof run.turnId === "string" && usableProcess(session)) {
                await session.process.client.request("turn/interrupt", { threadId: session.engineSessionId, turnId: run.turnId }, requestTimeoutMs).catch((cause) => {
                  ctx.log({ type: "interruptFailed", kind: mapCodexError(cause).kind });
                });
              }
              finish(session, run, cancelledFrame(run));
              return { cancelled: true };
            }
            case "respond": {
              const entry = pending.get(request.requestId);
              if (!entry) {
                ctx.log({ type: "lateRespond", requestId: request.requestId });
                return null;
              }
              pending.delete(request.requestId);
              entry.resolve(answerFor(entry, request.decision ?? {}));
              return null;
            }
            case "models": {
              return await withThrowaway(request, { codexPath: request.codexPath, env: request.env }, async (proc) => {
                let account;
                try {
                  const accountResult = await proc.client.request("account/read", {}, requestTimeoutMs);
                  if (accountResult?.account) {
                    account = {
                      ...(typeof accountResult.account.type === "string" ? { type: accountResult.account.type } : {}),
                      ...(typeof accountResult.account.planType === "string" ? { planType: accountResult.account.planType } : {}),
                      ...(typeof accountResult.account.email === "string" ? { email: accountResult.account.email } : {}),
                    };
                  }
                } catch (cause) {
                  ctx.log({ type: "accountReadFailed", kind: mapCodexError(cause).kind });
                }
                const data = [];
                let cursor;
                do {
                  const page = await proc.client.request("model/list", cursor ? { cursor } : {}, requestTimeoutMs);
                  data.push(...(page?.data ?? []));
                  cursor = page?.nextCursor ?? null;
                } while (cursor);
                return { ...codexModels({ data }), ...(account ? { account } : {}) };
              });
            }
            case "usage": {
              const session = request.sessionId === undefined ? undefined : sessionOf(request.sessionId);
              const usage = session?.lastUsage ?? null;
              let limits;
              try {
                const read = async (proc) => proc.client.request("account/rateLimits/read", null, requestTimeoutMs);
                const result = session && usableProcess(session)
                  ? await read(session.process)
                  : await withThrowaway(request, { codexPath: request.codexPath, env: request.env }, (proc) => read(proc));
                limits = codexLimits({ checkedAt: new Date().toISOString(), ...result });
              } catch (cause) {
                ctx.log({ type: "usageReadFailed", kind: mapCodexError(cause).kind });
                limits = { checkedAt: new Date().toISOString(), windows: [], unavailable: "probeFailed", message: "Codex did not answer the usage request." };
              }
              return { usage, limits };
            }
            case "rollback": {
              const session = sessionOf(request.sessionId);
              if (typeof request.toTurn !== "string" || request.toTurn === "") throw hostError("InvalidRollback", "rollback needs toTurn.", "invalid_request");
              if (!usableProcess(session)) throw hostError("CodexStopped", "Codex stopped. Open the session again.", "session_lost");
              const client = session.process.client;
              let loaded = true;
              try {
                const read = await client.request("thread/read", { threadId: session.engineSessionId, includeTurns: false }, requestTimeoutMs);
                loaded = read?.thread?.status?.type !== "notLoaded";
              } catch (cause) {
                const error = mapCodexError(cause);
                if (error.kind !== "session_lost") throw error;
                loaded = false;
              }
              if (!loaded) {
                await client.request("thread/resume", { threadId: session.engineSessionId, excludeTurns: true, ...threadParams(session) }, requestTimeoutMs);
              }
              // The turn immediately after toTurn is the one to revert before; toTurn itself is kept,
              // as T3's rollback does. The host remembers turns it started, so a resume in this process
              // writes the same one-page list as the recording; after a host restart it pages instead.
              const index = session.turns.indexOf(request.toTurn);
              let beforeTurnId;
              if (index >= 0) {
                beforeTurnId = session.turns[index + 1];
                await client.request(
                  "thread/turns/list",
                  { threadId: session.engineSessionId, cursor: null, limit: 100, sortDirection: "desc", itemsView: "summary" },
                  requestTimeoutMs,
                );
              } else {
                let cursor = null;
                let found = false;
                let laterTurnId;
                const visited = new Set();
                while (!found) {
                  if (visited.has(cursor)) throw hostError("CodexTurns", "Codex repeated a turns cursor.", "engine");
                  visited.add(cursor);
                  const page = await client.request(
                    "thread/turns/list",
                    { threadId: session.engineSessionId, cursor, limit: 100, sortDirection: "desc", itemsView: "summary" },
                    requestTimeoutMs,
                  );
                  for (const turn of page?.data ?? []) {
                    if (turn?.id === request.toTurn) {
                      found = true;
                      break;
                    }
                    laterTurnId = turn?.id;
                  }
                  if (!found && (page?.nextCursor === null || page?.nextCursor === undefined)) break;
                  cursor = page?.nextCursor ?? null;
                }
                if (!found) throw hostError("UnknownTurn", "Codex does not have that turn.", "invalid_request");
                beforeTurnId = laterTurnId;
              }
              if (beforeTurnId !== undefined) {
                await client.request("thread/revert", { threadId: session.engineSessionId, beforeTurnId }, requestTimeoutMs);
                if (!loaded) await client.request("thread/resume", { threadId: session.engineSessionId, excludeTurns: true, ...threadParams(session) }, requestTimeoutMs);
              }
              if (index >= 0) {
                session.turns = session.turns.slice(0, index + 1);
              } else {
                session.turns = [request.toTurn];
              }
              turnHistory.set(session.engineSessionId, [...session.turns]);
              return { ok: true };
            }
            case "fork": {
              const session = sessionOf(request.sessionId);
              if (!usableProcess(session)) throw hostError("CodexStopped", "Codex stopped. Open the session again.", "session_lost");
              const client = session.process.client;
              const result = await client.request(
                "thread/fork",
                { threadId: session.engineSessionId, excludeTurns: true, ...(typeof request.atTurn === "string" ? { lastTurnId: request.atTurn } : {}), ...threadParams(session) },
                requestTimeoutMs,
              );
              const threadId = result?.thread?.id;
              if (typeof threadId !== "string" || threadId === "") throw hostError("CodexThread", "Codex returned no forked thread id.", "engine");
              return { engineSessionId: threadId };
            }
            case "readHistory": {
              const session = sessionOf(request.sessionId);
              if (!usableProcess(session)) throw hostError("CodexStopped", "Codex stopped. Open the session again.", "session_lost");
              const client = session.process.client;
              let result = await client.request("thread/read", { threadId: session.engineSessionId, includeTurns: true }, requestTimeoutMs);
              if (result?.thread?.status?.type === "notLoaded") {
                await client.request("thread/resume", { threadId: session.engineSessionId, excludeTurns: true, ...threadParams(session) }, requestTimeoutMs);
                result = await client.request("thread/read", { threadId: session.engineSessionId, includeTurns: true }, requestTimeoutMs);
              }
              return { turns: Array.isArray(result?.thread?.turns) ? result.thread.turns : [] };
            }
            case "closeSession": {
              const session = sessions.get(request.sessionId);
              if (!session) return null;
              sessions.delete(request.sessionId);
              session.closing = true;
              if (session.process) {
                for (const [threadId, owner] of session.process.threadToSession) if (owner === session) session.process.threadToSession.delete(threadId);
              }
              refusePending(session, "cancel");
              if (session.run) {
                const run = session.run;
                stopAgents(session, "stopped");
                if (!run.held && typeof run.turnId === "string" && usableProcess(session)) {
                  await session.process.client.request("turn/interrupt", { threadId: session.engineSessionId, turnId: run.turnId }, requestTimeoutMs).catch(() => {});
                }
                finish(session, run, cancelledFrame(run));
              }
              if (session.process) {
                session.process.sessions.delete(session);
                if (session.process.sessions.size === 0) await closeProcess(session.process);
              }
              return null;
            }
            default:
              throw hostError("UnsupportedRequest", `unsupported request ${request.type}`, "unsupported");
          }
        },
        async close() {
          for (const session of sessions.values()) {
            session.closing = true;
            refusePending(session, "cancel");
            stopAgents(session, "stopped");
            if (session.run) finish(session, session.run, cancelledFrame(session.run));
          }
          sessions.clear();
          for (const proc of [...live]) await closeProcess(proc);
          processes.clear();
          live.clear();
        },
      };
    },
  };
}
