// The generic ACP family: one agent process per open session, spoken to over the Agent Client
// Protocol's newline-delimited JSON-RPC 2.0 on stdio. The core resolves which agent to run and
// where its binary is (`open.options.agent`); this host never downloads, installs or resolves a
// registry, and never fetches the network.
//
// The client, session runtime, config-option/model shaping, elicitation forms and stderr
// redaction are a plain-JavaScript port of T3 Code (MIT, Copyright (c) 2026 T3 Tools Inc., commit
// 9168ca73d02003557725430b515b44e1551544af): packages/effect-acp/src/{client,protocol,agent}.ts and
// packages/provider-acp/src/server/{AcpSessionRuntime,adapter,runtimeModel,sessionConfig,
// elicitationForm,stderr}.ts, with packages/provider-acp-registry/src/server/mcpTransport.ts for
// MCP transport selection. No ACP SDK is imported or copied.
//
// Deliberate differences from T3:
// - Policy stays in Rust. Every `session/request_permission` is a `permission` frame answered by
//   `respond`; an allow is only ever the agent's `allow_once` option (never `allow_always`), and
//   an ask the person denies is `reject_once`. Kiro's `autopilot` is therefore always written as
//   its supervised value, where T3 turns it on for a full-access thread.
// - The client advertises NO `fs` and NO `terminal` capability: the agent keeps using its own
//   tools under the core's confinement. T3 advertises and implements `fs/*` and `terminal/*`
//   (clientFileSystem, clientTerminals, clientPolicy); when an agent sends one anyway this host
//   answers JSON-RPC method-not-found and logs the method only.
// - No prompt wrapping. T3 sends its own `<t3_code_instructions>`/`<runtime_info>` blocks around
//   the person's text; the prompt here is the person's message and nothing else.
// - No background/steer native capabilities beyond what ACP itself provides: there is no
//   background event, and a steer cancels the running prompt and prompts the new text (the wire
//   T3's `message_steering` recording shows).
// - `models` returns Muse's `{models}` shape (see docs/engine-host-protocol.md), not T3's
//   provider catalog. The per-agent entries in acpAgents.mjs add the droid/omp/kimi quirks, the
//   Mistral Vibe model scope and Kiro's launch/config rules; T3's Kiro cloud-config and steering
//   note filters are not ported, so those updates stay engine events.
//
// See test/fixtures/NOTICE for the transcript notice and test/acp.test.mjs for what is replayed.

import { spawn as nodeSpawn } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { win32 } from "node:path";
import { createInterface } from "node:readline";
import { serializeError } from "../frame.mjs";
import {
  acpAgentHooks,
  KIRO_AGENT_CONFIG_ID,
  KIRO_AUTOPILOT_CONFIG_ID,
  KIRO_AUTOPILOT_OFF,
  KIRO_COMPACT_METHOD,
  KIRO_EFFORT_CONFIG_ID,
  KIRO_MODEL_CONFIG_ID,
  KIRO_STEER_METHOD,
} from "./acpAgents.mjs";

/** The ACP protocol generation this host speaks; also the `hello.sdk.version` string. */
export const ACP_PROTOCOL_VERSION = 2;

const START_TIMEOUT_MS = 20_000;
const REQUEST_TIMEOUT_MS = 30_000;
const CLOSE_REQUEST_TIMEOUT_MS = 2_000;
const CANCEL_GRACE_MS = 5_000;
const STDERR_TAIL_MAX_CHARS = 4_096;
const MAX_TRACKED_TOOL_CALLS = 256;

const owned = new WeakSet();
const hostError = (name, message, kind, cause) => {
  const error = Object.assign(new Error(message, cause ? { cause } : undefined), { name, kind });
  owned.add(error);
  return error;
};

const envValue = (env, name) => {
  const key = Object.keys(env).find((candidate) => candidate.toUpperCase() === name.toUpperCase());
  return key === undefined ? undefined : env[key];
};

/**
 * Windows launchers: resolve the agent with PATH and PATHEXT, and run a .cmd or .bat under
 * cmd.exe, because the child spawns without a shell. Elsewhere, and for a real executable, the
 * command is unchanged. This follows museLaunch, which the Muse family uses for `muse serve`.
 * (T3's registry refuses .cmd/.bat on Windows; the host follows the core, which resolves the
 * agent, and the Windows launcher instead.)
 */
export function acpLaunch(binary, env = process.env, { platform = process.platform, exists = existsSync } = {}) {
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

/**
 * The args a session passes to the agent. Kiro's V3 engine needs `acp --agent-engine=v3
 * --auth-method=cli` when the core leaves the list empty (T3's KIRO_ACP_ARGS); explicit args are
 * the core's choice and are never replaced. Other agents run with what the core gave them.
 */
export function acpAgentArgs(agent) {
  const args = Array.isArray(agent?.args) ? agent.args : [];
  const launchArgs = acpAgentHooks(agent?.id).launchArgs;
  if (Array.isArray(launchArgs) && launchArgs.length > 0 && args.length === 0) return [...launchArgs];
  return args;
}

/** The default child: a real process, one per open session. Tests inject a fake instead. */
export function spawnAcpAgent({ command, args, cwd, env }) {
  const child = nodeSpawn(command, args, { cwd, env, stdio: ["pipe", "pipe", "pipe"], windowsHide: true, shell: false });
  let exited = false;
  const exit = new Promise((resolve) => {
    child.on("exit", (code, signal) => {
      exited = true;
      resolve({ code, signal });
    });
    // A failed spawn (ENOENT) emits `error` and no `exit`; closing must not wait forever.
    child.on("error", () => {
      if (exited) return;
      exited = true;
      resolve({ code: null, signal: null });
    });
  });
  return {
    write(line) {
      child.stdin.write(line.endsWith("\n") ? line : `${line}\n`);
    },
    onLine(handler) {
      createInterface({ input: child.stdout }).on("line", handler);
    },
    onStderr(handler) {
      child.stderr.on("data", (chunk) => handler(String(chunk)));
    },
    onError(handler) {
      child.on("error", handler);
    },
    exited: exit,
    async close() {
      if (!exited) child.kill();
      const timer = setTimeout(() => {
        if (!exited) child.kill("SIGKILL");
      }, 2_000);
      timer.unref?.();
      const result = await Promise.race([exit, new Promise((resolve) => setTimeout(() => resolve({ code: null, signal: "SIGKILL" }), 3_000).unref?.())]);
      clearTimeout(timer);
      return result;
    },
  };
}

const ACP_STDERR_MAX_CHARS = STDERR_TAIL_MAX_CHARS;
const PAIRING_URL_PATTERN = /https?:\/\/[^\s]*\/pair#[^\s]*/gi;
const SIGN_IN_URL_PATTERN = /https?:\/\/accounts\.google\.com[^\s]*/gi;
const BEARER_TOKEN_PATTERN = /\bBearer\s+[A-Za-z0-9._\-+=/]+/gi;
const BASIC_AUTH_PATTERN = /\bAuthorization:\s*Basic\s+\S+/gi;
const API_KEY_HEADER_PATTERN = /\bx-api-key:\s*\S+/gi;
const SECRET_TOKEN_PATTERN = /\b(?:sk-[A-Za-z0-9][A-Za-z0-9_-]{7,}|ghp_[A-Za-z0-9]+|xox[a-zA-Z]-[A-Za-z0-9-]+)\b/g;

const appendStderrTail = (current, chunk) => {
  const next = `${current}${chunk}`;
  return next.length <= ACP_STDERR_MAX_CHARS ? next : next.slice(-ACP_STDERR_MAX_CHARS);
};

/** T3 provider-acp/stderr.ts: what an error may say about what the agent printed on stderr. */
export function acpStderrExcerpt(text, homeDirectory = homedir(), environment = process.env) {
  let result = String(text ?? "").replaceAll("\0", "");
  const homes = [environment.HOME, environment.USERPROFILE, homeDirectory].filter((value) => typeof value === "string" && value.length > 1);
  for (const home of new Set(homes)) result = result.split(home).join("~");
  result = result
    .replace(PAIRING_URL_PATTERN, "[pairing-url]")
    .replace(SIGN_IN_URL_PATTERN, "[sign-in-url]")
    .replace(BEARER_TOKEN_PATTERN, "Bearer [redacted]")
    .replace(BASIC_AUTH_PATTERN, "Authorization: Basic [redacted]")
    .replace(API_KEY_HEADER_PATTERN, "x-api-key: [redacted]")
    .replace(SECRET_TOKEN_PATTERN, "[redacted]");
  // Never let stderr become the frame: a short excerpt only, and the callers add it only to a
  // process-exit error. It can hold prompt text, so tests assert that raw tokens never survive.
  return result.trim().slice(-512);
}

const AUTH = /not logged in|log ?in|sign ?in|unauthori[sz]ed|authentication|credential|api key/i;
const LIMIT = /rate.?limit|usage limit|quota|too many requests|429/i;
const OVERLOAD = /overload|temporarily unavailable|\b529\b|\b503\b/i;
const NETWORK = /\beconn\w*|network|fetch failed|socket|connection (?:lost|reset|refused|closed)|\bepipe\b|timed? ?out/i;
const SESSION_LOST = /session (?:not found|lost|expired|unknown|does not exist|no longer)/i;
const CANCELLED = /cancel(?:l)?ed|interrupted|aborted/i;

/**
 * Maps a thrown JSON-RPC or transport error to the protocol's closed `kind` set, keeping a retry
 * hint when the core can act on it. The agent's `data` is never copied into a cause (it can hold
 * the prompt).
 */
function acpError(cause, fallback = "engine") {
  if (owned.has(cause)) return cause;
  const name = typeof cause?.name === "string" ? cause.name : "Error";
  const message = typeof cause?.message === "string" ? cause.message : String(cause);
  const code = typeof cause?.code === "number" ? cause.code : undefined;
  const status = typeof cause?.status === "number" ? cause.status : typeof cause?.data?.http_status === "number" ? cause.data.http_status : undefined;
  const detail = typeof cause?.data?.message === "string" ? cause.data.message : "";
  const text = `${message} ${detail}`;
  let kind = fallback;
  let retryWhy;
  if (code === -32000) kind = "auth";
  else if (code === -32601) kind = "unsupported";
  else if (code === -32602 || status === 400 || status === 422) kind = "invalid_request";
  else if (code === -32800) kind = "cancelled";
  else if (status === 401 || status === 403) kind = "auth";
  else if (status === 429) {
    kind = "rate_limit";
    retryWhy = "rate_limit";
  } else if (status === 502 || status === 503 || status === 504) {
    kind = "network";
    retryWhy = "network";
  } else if (SESSION_LOST.test(text)) kind = "session_lost";
  else if (CANCELLED.test(text) && fallback !== "engine") kind = "cancelled";
  else if (AUTH.test(text)) kind = "auth";
  else if (LIMIT.test(text)) {
    kind = "rate_limit";
    retryWhy = "rate_limit";
  } else if (OVERLOAD.test(text)) {
    kind = "network";
    retryWhy = "overloaded";
  } else if (NETWORK.test(text)) {
    kind = "network";
    retryWhy = "network";
  } else if (CANCELLED.test(text)) kind = "cancelled";
  const error = Object.assign(new Error(message), { name, kind, ...(code === undefined ? {} : { code }), ...(status === undefined ? {} : { status }), ...(retryWhy ? { retryWhy } : {}) });
  owned.add(error);
  return error;
}

const retryFor = (error) =>
  error?.retryWhy === "rate_limit"
    ? { why: "rate_limit" }
    : error?.retryWhy === "network"
      ? { why: "network" }
      : error?.retryWhy === "overloaded"
        ? { why: "overloaded" }
        : undefined;

/** ACP `Usage` to the host's token counters, T3's clamping rules (subset of its parent). */
export function acpUsage(usage) {
  if (!usage || typeof usage !== "object") return undefined;
  const finite = (value) => typeof value === "number" && Number.isFinite(value);
  if (!finite(usage.inputTokens) && !finite(usage.outputTokens)) return undefined;
  const inputTokens = finite(usage.inputTokens) ? usage.inputTokens : 0;
  const outputTokens = finite(usage.outputTokens) ? usage.outputTokens : 0;
  const clamp = (value, parent) => (finite(value) ? Math.min(value, parent) : undefined);
  const cachedInputTokens = clamp(usage.cachedReadTokens, inputTokens);
  const cacheCreationTokens = clamp(usage.cachedWriteTokens, inputTokens - (cachedInputTokens ?? 0));
  const reasoningTokens = clamp(usage.thoughtTokens, outputTokens);
  return {
    inputTokens,
    outputTokens,
    ...(cachedInputTokens === undefined ? {} : { cachedInputTokens }),
    ...(cacheCreationTokens === undefined ? {} : { cacheCreationTokens }),
    ...(reasoningTokens === undefined ? {} : { reasoningTokens }),
  };
}

// ---------------------------------------------------------------------------------------------
// Config options and models (T3 runtimeModel.ts / sessionConfig.ts, in the host's Muse shape).
// ---------------------------------------------------------------------------------------------

const isRecord = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value) => (typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined);

function flattenSelectChoices(options) {
  if (!Array.isArray(options)) return [];
  return options.flatMap((candidate) => {
    if (!isRecord(candidate)) return [];
    if (typeof candidate.value === "string") return [candidate];
    return Array.isArray(candidate.options) ? candidate.options.filter((entry) => isRecord(entry) && typeof entry.value === "string") : [];
  });
}

function findModelConfigOption(configOptions) {
  return Array.isArray(configOptions)
    ? configOptions.find((option) => option?.category === "model" && option.type === "select" && typeof option.id === "string" && option.id.trim().length > 0)
    : undefined;
}

/**
 * ACP v2 names a config option's id `configId`; the compat shape T3 reads (and this host keeps)
 * calls it `id`. Unknown option kinds are dropped rather than failing the whole setup.
 */
function normalizeConfigOptions(options) {
  if (!Array.isArray(options)) return undefined;
  return options.flatMap((option) => {
    if (!isRecord(option)) return [];
    if (option.type !== "select" && option.type !== "boolean") return [];
    const id = typeof option.id === "string" ? option.id : typeof option.configId === "string" ? option.configId : undefined;
    return id === undefined ? [] : [{ ...option, id }];
  });
}

/** Mistral Vibe 3.1 sends its reasoning level under `thinking`, which ACP never defined. */
function findThoughtLevelConfigOption(configOptions) {
  if (!Array.isArray(configOptions)) return undefined;
  return (
    configOptions.find((option) => option?.category === "thought_level" && option.type === "select") ??
    configOptions.find((option) => option?.category === "reasoning" && option.type === "select") ??
    configOptions.find((option) => option?.category === "thinking" && option.type === "select")
  );
}

function collectConfigOptionValues(option) {
  return option?.type === "select" ? flattenSelectChoices(option.options).map((choice) => choice.value) : [];
}

const KIRO_EFFORT_LABELS = Object.freeze({
  none: "None",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra High",
  max: "Max",
});

/**
 * The model catalog in Muse's `{models}` shape: ids, names, a default, efforts and a default effort.
 *
 * `agentId` carries the per-agent catalog rules from acpAgents.mjs: Mistral Vibe's `thinking`
 * option describes only the session's current model, and Kiro sends each model's levels under
 * `_meta.kiro` (effortLevels/defaultEffortLevel). The generic entry keeps the session levels on
 * every model, as before.
 */
export function acpModelCatalog(setup = {}, agentId = "default") {
  const hooks = acpAgentHooks(agentId);
  const configOptions = Array.isArray(setup.configOptions) ? setup.configOptions : [];
  const modelState = isRecord(setup.models) ? setup.models : null;
  const modelOption = findModelConfigOption(configOptions);
  const currentModelId = modelOption !== undefined ? text(modelOption.currentValue) : text(modelState?.currentModelId);
  const candidates = modelOption !== undefined
    ? flattenSelectChoices(modelOption.options).map((choice) => ({ id: choice.value, name: choice.name, description: choice.description, meta: choice._meta }))
    : (Array.isArray(modelState?.availableModels) ? modelState.availableModels : []).map((model) => ({ id: model?.modelId, name: model?.name, description: model?.description, meta: model?._meta }));
  const effortOption = findThoughtLevelConfigOption(configOptions);
  const sessionEfforts = effortOption === undefined ? [] : flattenSelectChoices(effortOption.options).map((choice) => ({ id: choice.value, label: text(choice.name) ?? choice.value, ...(text(choice.description) ? { description: text(choice.description) } : {}) }));
  const sessionEffort = text(effortOption?.currentValue);
  const seen = new Set();
  const models = [];
  for (const candidate of candidates) {
    const id = text(candidate.id);
    if (id === undefined || seen.has(id)) continue;
    seen.add(id);
    const meta = candidate.meta ?? (Array.isArray(modelState?.availableModels) ? modelState.availableModels.find((model) => model?.modelId === id)?._meta : undefined);
    const kiro = isRecord(meta) && isRecord(meta.kiro) ? meta.kiro : undefined;
    let efforts;
    let defaultEffort;
    if (kiro !== undefined && (kiro.hasEffort === false || Array.isArray(kiro.effortLevels))) {
      efforts = kiro.hasEffort === false || !Array.isArray(kiro.effortLevels)
        ? []
        : kiro.effortLevels
            .filter((level) => typeof level === "string" && level.length > 0)
            .map((level) => ({ id: level, label: KIRO_EFFORT_LABELS[level] ?? level }));
      defaultEffort = text(kiro.defaultEffortLevel);
    } else if (effortOption === undefined) {
      efforts = [];
    } else if (isRecord(meta) && meta.supportsReasoningEffort === false) {
      efforts = [];
    } else if (isRecord(meta) && Array.isArray(meta.reasoningEfforts)) {
      efforts = meta.reasoningEfforts
        .filter((level) => isRecord(level) && typeof level.value === "string" && level.value.length > 0)
        .map((level) => ({ id: level.value, label: text(level.label) ?? level.value }));
      defaultEffort = text(meta.reasoningEffort);
    } else if (hooks.modelEfforts === "current-model" && id !== currentModelId) {
      // Mistral Vibe 3.1 sends its `thinking` levels for the session's model only.
      efforts = [];
    } else {
      efforts = sessionEfforts;
      defaultEffort = sessionEffort;
    }
    if (efforts.length > 0 && !efforts.some((effort) => effort.id === defaultEffort)) defaultEffort = undefined;
    models.push({
      id,
      name: text(candidate.name) ?? id,
      isDefault: id === currentModelId,
      efforts: efforts.map((effort) => ({ ...effort, ...(effort.id === defaultEffort ? { isDefault: true } : {}) })),
      ...(defaultEffort === undefined ? {} : { defaultEffort }),
    });
  }
  return { models };
}

// ---------------------------------------------------------------------------------------------
// Elicitation forms (T3 provider-acp/elicitationForm.ts, ported to plain JS).
// ---------------------------------------------------------------------------------------------

const formText = (value) => (typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined);

function choiceOptions(entries, toOption) {
  if (!Array.isArray(entries) || entries.length === 0) return undefined;
  const options = entries.map(toOption);
  return options.every((option) => option !== undefined) ? options : undefined;
}

function enumOptions(entries) {
  if (!Array.isArray(entries) || entries.length === 0 || !entries.every((value) => typeof value === "string")) return undefined;
  const counts = new Map();
  for (const value of entries) counts.set(value.trim(), (counts.get(value.trim()) ?? 0) + 1);
  const options = entries.map((value) => {
    const plain = value.trim();
    const label = plain !== "" && counts.get(plain) === 1 ? plain : JSON.stringify(value);
    return { label, description: label, value };
  });
  return new Set(options.map((option) => option.label)).size === options.length
    ? options
    : options.map((option) => {
        const label = JSON.stringify(option.value);
        return { ...option, label, description: label };
      });
}

function titledOption(entry) {
  if (!isRecord(entry)) return undefined;
  const label = formText(entry.title) ?? formText(entry.const);
  return typeof entry.const === "string" && label !== undefined ? { label, description: formText(entry.description) ?? label, value: entry.const } : undefined;
}

function parseFormField(property) {
  if (!isRecord(property)) return undefined;
  switch (property.type) {
    case "string": {
      if ((property.enum ?? property.oneOf ?? undefined) === undefined) return { field: { kind: "text" }, options: [] };
      const options = Array.isArray(property.oneOf) ? choiceOptions(property.oneOf, titledOption) : enumOptions(property.enum);
      return options === undefined ? undefined : { field: { kind: "choice", values: new Set(options.map(optionValue)) }, options };
    }
    case "array": {
      const items = isRecord(property.items) ? property.items : undefined;
      const options =
        items?.type === "string"
          ? enumOptions(items.enum)
          : items !== undefined && items.type === undefined
            ? choiceOptions(items.anyOf, titledOption)
            : undefined;
      if (options === undefined) return undefined;
      const values = new Set(options.map(optionValue));
      const minItems = typeof property.minItems === "number" ? property.minItems : 0;
      const maxItems = typeof property.maxItems === "number" ? property.maxItems : Infinity;
      if (maxItems === 0 || minItems > maxItems || minItems > values.size) return undefined;
      return { field: { kind: "multiChoice", values, minItems, maxItems }, options };
    }
    case "boolean":
      return {
        field: { kind: "boolean" },
        options: [
          { label: "true", description: "Yes", value: "true" },
          { label: "false", description: "No", value: "false" },
        ],
      };
    case "number":
    case "integer":
      return { field: { kind: "number", integer: property.type === "integer" }, options: [] };
    default:
      return undefined;
  }
}

const optionValue = (option) => option.value ?? option.label;

/** Types answers as the schema declares them; unknown keys and options are dropped. */
function typedAnswer(field, answer) {
  switch (field.kind) {
    case "text":
      return typeof answer === "string" ? answer : undefined;
    case "boolean":
      if (typeof answer === "boolean") return answer;
      return answer === "true" ? true : answer === "false" ? false : undefined;
    case "number": {
      const parsed = typeof answer === "number" ? answer : typeof answer === "string" && answer.trim().length > 0 ? Number(answer.trim()) : Number.NaN;
      if (!Number.isFinite(parsed)) return undefined;
      return field.integer && !Number.isInteger(parsed) ? undefined : parsed;
    }
    case "choice":
      return typeof answer === "string" && field.values.has(answer) ? answer : undefined;
    case "multiChoice": {
      const list = typeof answer === "string" ? answer.split(",").map((entry) => entry.trim()).filter(Boolean) : Array.isArray(answer) ? answer : undefined;
      if (list === undefined) return undefined;
      const selected = [...new Set(list.filter((entry) => typeof entry === "string" && field.values.has(entry)))];
      if (selected.length === 0 && list.length > 0) return undefined;
      return selected.length >= field.minItems && selected.length <= field.maxItems ? selected : undefined;
    }
    default:
      return undefined;
  }
}

/**
 * The generic ACP form parser. `questions` is the host protocol's question list keyed by the
 * field id, and `content` types the answers the core sends (keyed by the question text or the
 * field id) into the elicitation content, or undefined when a required field has no valid answer.
 */
export function parseElicitationForm(requestedSchema, message, { allowOther = false } = {}) {
  const schema = isRecord(requestedSchema) ? requestedSchema : undefined;
  const required = new Set(Array.isArray(schema?.required) ? schema.required.filter((key) => typeof key === "string") : []);
  const fields = new Map();
  const parsedFields = [];
  for (const [id, value] of Object.entries(isRecord(schema?.properties) ? schema.properties : {})) {
    const property = isRecord(value) ? value : undefined;
    const parsed = property === undefined ? undefined : parseFormField(property);
    if (property === undefined || parsed === undefined || formText(id) !== id) return undefined;
    fields.set(id, parsed.field);
    parsedFields.push({ id, property, options: parsed.options });
  }
  const companions = new Map();
  for (const [id, field] of fields) {
    const companionId = `${id}__other`;
    if (field.kind === "choice" && fields.get(companionId)?.kind === "text" && !required.has(id) && !required.has(companionId)) companions.set(id, companionId);
  }
  const folded = new Set(companions.values());
  const questions = [];
  for (const { id, property, options } of parsedFields) {
    if (folded.has(id)) continue;
    const kind = fields.get(id)?.kind;
    questions.push({
      id,
      header: formText(property.title) ?? `Question ${questions.length + 1}`,
      question: formText(property.description) ?? message,
      options,
      ...(kind === "text" || kind === "number" || companions.has(id) || (allowOther && kind === "choice") ? {} : { allowCustomAnswer: false }),
      ...(kind === "multiChoice" ? { multiSelect: true } : {}),
      ...(kind === "text" && !required.has(id) ? { required: false } : {}),
    });
  }
  return {
    questions,
    content: (answers) => {
      const entries = [];
      for (const [id, field] of fields) {
        if (folded.has(id)) continue;
        const answer = Object.hasOwn(answers, id) ? answers[id] : undefined;
        const typed = answer === undefined ? undefined : typedAnswer(field, answer);
        const companionId = companions.get(id);
        if (typed !== undefined) entries.push([id, typed]);
        else if (allowOther && field.kind === "choice" && typeof answer === "string" && answer.trim() !== "") entries.push([id, answer]);
        else if (companionId !== undefined && typeof answer === "string") entries.push([companionId, answer]);
        else if (required.has(id)) return undefined;
      }
      return Object.fromEntries(entries);
    },
  };
}

/** The core's answers (keyed by question text, then field id) typed for the form. */
function formAnswers(form, decision) {
  const raw = isRecord(decision?.answers) ? decision.answers : {};
  const byId = {};
  for (const question of form.questions) {
    const answer = Object.hasOwn(raw, question.question) ? raw[question.question] : Object.hasOwn(raw, question.id) ? raw[question.id] : undefined;
    if (answer === undefined) continue;
    const options = Array.isArray(question.options) ? question.options : [];
    // The protocol's answers carry the labels people saw and join multi-selections with commas;
    // the schema is typed by the options' underlying values (T3's typedAnswer).
    const toValue = (token) => options.find((option) => option?.label === token)?.value ?? token;
    if (question.multiSelect) {
      byId[question.id] = typeof answer === "string" ? answer.split(",").map((entry) => entry.trim()).filter(Boolean).map(toValue) : Array.isArray(answer) ? answer.map((entry) => (typeof entry === "string" ? toValue(entry) : entry)) : answer;
    } else if (typeof answer === "string") {
      byId[question.id] = toValue(answer);
    } else {
      byId[question.id] = answer;
    }
  }
  return byId;
}

// ---------------------------------------------------------------------------------------------
// The connection: newline JSON-RPC 2.0 over the agent's stdio.
// ---------------------------------------------------------------------------------------------

function connect({ child, ctx, onRequest, onNotification, onCancel, onExit }) {
  let nextId = 1;
  let closed = false;
  let failure;
  const pending = new Map();

  const write = (frame) => {
    if (closed) throw hostError("AcpClosed", "The agent process is closed.", "engine");
    child.write(JSON.stringify(frame));
  };

  const request = (method, params, { timeoutMs = REQUEST_TIMEOUT_MS } = {}) => {
    if (failure) return Promise.reject(failure);
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const settle = { method, resolve: undefined, reject: undefined };
      let timer;
      const done = (fn) => (value) => {
        if (timer !== undefined) clearTimeout(timer);
        pending.delete(id);
        fn(value);
      };
      settle.resolve = done(resolve);
      settle.reject = done(reject);
      pending.set(id, settle);
      if (timeoutMs > 0) {
        timer = setTimeout(() => {
          pending.delete(id);
          reject(hostError("AcpTimeout", `The agent did not answer ${method}.`, "network"));
        }, timeoutMs);
        timer.unref?.();
      }
      try {
        write({ jsonrpc: "2.0", id, method, params });
      } catch (cause) {
        pending.delete(id);
        if (timer !== undefined) clearTimeout(timer);
        reject(cause);
      }
    });
  };

  const notify = (method, params) => write({ jsonrpc: "2.0", method, params });
  const respond = (id, result) => write({ jsonrpc: "2.0", id, result });
  const respondError = (id, code, message) => write({ jsonrpc: "2.0", id, error: { code, message } });

  const failAll = (cause) => {
    failure ??= cause;
    for (const [id, entry] of pending) {
      pending.delete(id);
      entry.reject(cause);
    }
  };

  const closedPromise = child.exited.then((exit) => {
    if (closed) return exit;
    closed = true;
    // Mark the session before rejecting pending requests, so the prompt's rejection sees a
    // process that is gone and the run ends `broken`.
    onExit?.(exit);
    failAll(hostError("AcpExited", `The agent process exited (${exit?.code ?? exit?.signal ?? "unknown"}).`, "engine"));
    return exit;
  });
  // A child that only has an `error` event (ENOENT) also settles `exited` in most implementations.
  child.onError((cause) => {
    if (closed) return;
    closed = true;
    failAll(acpError(cause, "engine"));
  });

  const onLine = (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      ctx.log({ type: "malformedAgentLine" });
      return;
    }
    if (!isRecord(message)) {
      ctx.log({ type: "malformedAgentLine" });
      return;
    }
    try {
      if (message.method !== undefined && (message.id === undefined || message.id === "")) {
        if (message.method === "session/cancel") onCancel?.(message.params);
        else onNotification(message.method, isRecord(message.params) ? message.params : {});
        return;
      }
      if (message.method !== undefined) {
        onRequest(message);
        return;
      }
      if (message.id !== undefined) {
        const entry = pending.get(message.id);
        if (!entry) {
          ctx.log({ type: "lateAgentResponse" });
          return;
        }
        pending.delete(message.id);
        if (message.error !== undefined) {
          const error = acpError({ name: "AcpRequestError", ...(isRecord(message.error) ? message.error : { message: String(message.error) }) });
          entry.reject(error);
        } else {
          entry.resolve(message.result);
        }
        return;
      }
      ctx.log({ type: "malformedAgentLine" });
    } catch (cause) {
      ctx.log({ type: "agentFrameFailed", message: String(cause?.message ?? cause) });
    }
  };

  child.onLine(onLine);
  return { request, notify, respond, respondError, failAll, closed: closedPromise, get failed() { return failure; } };
}

// ---------------------------------------------------------------------------------------------
// Resilience checks for `session/update` payloads. A malformed one is dropped and logged by
// method only (the recorded malformed `usage_update`); unknown update kinds pass through, as the
// protocol keeps the engine's event untouched.
// ---------------------------------------------------------------------------------------------

function wellFormedUpdate(update) {
  if (!isRecord(update) || typeof update.sessionUpdate !== "string" || update.sessionUpdate.length === 0) return false;
  switch (update.sessionUpdate) {
    case "usage_update":
      return typeof update.used === "number" && Number.isFinite(update.used) && typeof update.size === "number" && Number.isFinite(update.size);
    case "tool_call":
    case "tool_call_update":
      return typeof update.toolCallId === "string" && update.toolCallId.length > 0;
    case "plan":
      return Array.isArray(update.entries);
    case "config_option_update":
      return Array.isArray(update.configOptions);
    case "agent_message_chunk":
    case "agent_thought_chunk":
    case "user_message_chunk":
      return isRecord(update.content);
    default:
      return true;
  }
}

function normalizeInitialize(result) {
  const v2 = isRecord(result) && result.info !== undefined;
  if (!v2) {
    return {
      generation: 1,
      protocolVersion: result?.protocolVersion,
      agentCapabilities: isRecord(result?.agentCapabilities) ? result.agentCapabilities : {},
      authMethods: Array.isArray(result?.authMethods) ? result.authMethods : [],
    };
  }
  const session = isRecord(result.capabilities?.session) ? result.capabilities.session : undefined;
  const prompt = isRecord(session?.prompt) ? session.prompt : {};
  const mcp = isRecord(session?.mcp) ? session.mcp : {};
  // Kiro keeps its extension methods under `_meta.kiro` on the agent capabilities, even when it
  // otherwise answers with the v1 shape. Preserve the meta either way.
  const meta = isRecord(result.agentCapabilities?._meta) ? result.agentCapabilities._meta : isRecord(result.capabilities?._meta) ? result.capabilities._meta : undefined;
  return {
    generation: 2,
    protocolVersion: result.protocolVersion,
    agentCapabilities: {
      ...(meta === undefined ? {} : { _meta: meta }),
      loadSession: session !== undefined,
      promptCapabilities: {
        image: prompt.image != null,
        audio: prompt.audio != null,
        embeddedContext: prompt.embeddedContext != null,
      },
      mcpCapabilities: {
        stdio: mcp.stdio != null,
        http: mcp.http != null,
        acp: mcp.acp != null,
      },
      ...(session === undefined
        ? {}
        : {
            sessionCapabilities: {
              list: {},
              resume: {},
              close: {},
              ...(session.delete == null ? {} : { delete: {} }),
              ...(session.fork == null ? {} : { fork: {} }),
              ...(session.additionalDirectories == null ? {} : { additionalDirectories: {} }),
            },
          }),
    },
    authMethods: Array.isArray(result.authMethods)
      ? result.authMethods.flatMap((method) => {
          const id = text(method?.methodId) ?? text(method?.id);
          return id === undefined ? [] : [{ ...method, id }];
        })
      : [],
  };
}

const headerPairs = (value) => {
  if (Array.isArray(value)) return value.filter((entry) => isRecord(entry) && typeof entry.name === "string" && typeof entry.value === "string");
  if (!isRecord(value)) return [];
  return Object.entries(value).filter(([, header]) => typeof header === "string").map(([name, header]) => ({ name, value: header }));
};

const envPairs = (value) => {
  if (Array.isArray(value)) return value.filter((entry) => isRecord(entry) && typeof entry.name === "string" && typeof entry.value === "string");
  if (!isRecord(value)) return [];
  return Object.entries(value).filter(([, env]) => typeof env === "string").map(([name, env]) => ({ name, value: env }));
};

/** `options.mcpServers` to ACP `mcpServers`, picking only transports the agent advertised. */
function acpMcpServers(session, servers, log) {
  const capabilities = session.caps?.mcpCapabilities ?? {};
  const out = [];
  for (const server of Array.isArray(servers) ? servers : []) {
    if (!isRecord(server) || typeof server.name !== "string" || server.name.length === 0) continue;
    const kind = server.type ?? (typeof server.url === "string" ? "http" : typeof server.serverId === "string" ? "acp" : typeof server.command === "string" ? "stdio" : undefined);
    if (kind === "http") {
      if (capabilities.http !== true || typeof server.url !== "string") {
        log({ type: "mcpServerSkipped", name: server.name, transport: "http" });
        continue;
      }
      const headers = headerPairs(server.headers);
      out.push({
        ...(server.type === undefined && session.generation !== 2 ? {} : { type: "http" }),
        name: server.name,
        url: server.url,
        ...(headers.length > 0 ? { headers } : {}),
      });
    } else if (kind === "acp") {
      if (capabilities.acp !== true || typeof server.serverId !== "string") {
        log({ type: "mcpServerSkipped", name: server.name, transport: "acp" });
        continue;
      }
      out.push({ ...(server.type === undefined && session.generation !== 2 ? {} : { type: "acp" }), name: server.name, serverId: server.serverId });
    } else if (kind === "stdio") {
      if (typeof server.command !== "string") continue;
      const args = Array.isArray(server.args) ? server.args.filter((arg) => typeof arg === "string") : [];
      const env = envPairs(server.env);
      out.push({
        ...(server.type === undefined && session.generation !== 2 ? {} : { type: "stdio" }),
        name: server.name,
        command: server.command,
        ...(args.length > 0 ? { args } : {}),
        ...(env.length > 0 ? { env } : {}),
      });
    } else {
      log({ type: "mcpServerSkipped", name: server.name, transport: "unknown" });
    }
  }
  return out;
}

const modeStateOf = (setup) => {
  const modeOption = Array.isArray(setup?.configOptions) ? setup.configOptions.find((option) => option?.category === "mode" && option.type === "select" && Array.isArray(option.options)) : undefined;
  if (modeOption !== undefined) {
    const currentModeId = text(modeOption.currentValue);
    return currentModeId === undefined ? undefined : { currentModeId };
  }
  const modes = setup?.modes;
  return isRecord(modes) && text(modes.currentModeId) !== undefined ? modes : undefined;
};

const isKiro = (session) => session.agent?.id === "kiro";
const isKiroCompactCommand = (value) => /^\/compact(?:\s|$)/.test(value.trim());
const kiroExtensionMethods = (session) =>
  Array.isArray(session.init?.agentCapabilities?._meta?.kiro?.extensionMethods)
    ? session.init.agentCapabilities._meta.kiro.extensionMethods.filter((method) => typeof method === "string")
    : [];
const kiroAdvertises = (session, method) => kiroExtensionMethods(session).includes(method);

// ---------------------------------------------------------------------------------------------

export function acpFamily({ spawn = spawnAcpAgent, startTimeoutMs = START_TIMEOUT_MS, requestTimeoutMs = REQUEST_TIMEOUT_MS, cancelGraceMs = CANCEL_GRACE_MS } = {}) {
  return {
    name: "acp",
    sdk: { name: "agent-client-protocol", version: String(ACP_PROTOCOL_VERSION) },
    capabilities: ["steer", "models"],
    create(ctx) {
      const sessions = new Map();
      const pending = new Map(); // host requestId -> {session, kind, jsonrpcId, run, form, generation}
      let counter = 0;

      const emit = (run, frame) => {
        if (!run) return ctx.send(frame);
        if (run.done) return; // after runDone the host sends nothing more for that sendId
        if (!run.open) run.buffer.push(frame);
        else ctx.send(frame);
      };
      const forSession = (session, frame) => {
        const run = session.run;
        if (run && !run.done) emit(run, frame);
        else ctx.send(frame);
      };
      const sendIdOf = (session) => (session.run && !session.run.done ? { sendId: session.run.sendId } : {});
      const sessionOf = (id) => {
        const session = sessions.get(id);
        if (!session) throw hostError("UnknownSession", "unknown session", "invalid_request");
        return session;
      };
      const runOf = (sendId) => {
        for (const session of sessions.values()) if (session.run?.sendId === sendId && !session.run.done) return session;
        return undefined;
      };

      const answerAsk = (entry, result) => {
        pending.delete(entry.requestId);
        try {
          entry.session.connection.respond(entry.jsonrpcId, result);
        } catch (cause) {
          ctx.log({ type: "respondFailed", requestId: entry.requestId, message: String(cause?.message ?? cause) });
        }
      };

      const refuseAsks = (session, run) => {
        for (const entry of [...pending.values()]) {
          if (entry.session !== session) continue;
          if (run !== undefined && entry.run !== run) continue;
          answerAsk(entry, entry.kind === "question" ? { action: "cancel" } : { outcome: { outcome: "cancelled" } });
        }
      };

      const finish = (session, run, frame) => {
        if (!run || run.done) return;
        run.done = true;
        refuseAsks(session, run);
        if (session.run === run) session.run = null;
        if (!run.open) {
          run.buffer.push({ type: "runDone", sendId: run.sendId, ...frame });
        } else {
          ctx.send({ type: "runDone", sendId: run.sendId, ...frame });
        }
        run.settledResolve?.();
      };

      const failureFrame = (error, { broken = false } = {}) => {        const cause = serializeError(error);
        return {
          ok: false,
          result: {
            reason: "failed",
            threadDisposition: broken ? "broken" : "reusable",
            ...(retryFor(error) ? { retry: retryFor(error) } : {}),
          },
          error: { method: "send", cause: { ...cause, ...(typeof error.code === "number" ? { code: error.code } : {}), ...(typeof error.status === "number" ? { status: error.status } : {}) } },
        };
      };

      /**
       * Kiro answers a prompt error with -32000 (normally ACP's auth-required code), e.g. "The
       * model 'x' is not available". A prompt failure is the engine's, not a sign-in: keep the
       * message Kiro meant for the person and do not send the core to the sign-in path.
       */
      const promptError = (session, cause) => {
        const error = acpError(cause, "engine");
        return isKiro(session) && error.kind === "auth"
          ? hostError("KiroPromptError", error.message, "engine")
          : error;
      };

      const settleRun = (session, run, result) => {
        if (run.done || session.run !== run) return;
        const usage = acpUsage(result?.usage);
        const stopReason = result?.stopReason;
        const delegated = { end_turn: { ok: true, reason: "completed" }, cancelled: { ok: true, reason: "cancelled" } }[stopReason];
        const failed = stopReason === "max_tokens" || stopReason === "max_turn_requests" ? "maxTurns" : stopReason === "refusal" ? "refused" : undefined;
        if (delegated) {
          finish(session, run, { ok: true, result: { reason: delegated.reason, threadDisposition: "reusable", ...(usage ? { usage } : {}) } });
          return;
        }
        if (failed) {
          const error = hostError("AcpStopReason", `The agent stopped before finishing (${stopReason}).`, "engine");
          finish(session, run, {
            ...failureFrame(error),
            result: { reason: failed, threadDisposition: "reusable", ...(usage ? { usage } : {}) },
          });
          return;
        }
        if (stopReason === "error") {
          const error = promptError(session, {
            name: "AcpPromptError",
            message: typeof result?.error?.message === "string" ? result.error.message : "The agent ended the turn with an error.",
            ...(isRecord(result?.error?.data) ? { data: result.error.data } : {}),
          });
          finish(session, run, { ...failureFrame(error), result: { reason: "failed", threadDisposition: "reusable", ...(usage ? { usage } : {}) } });
          return;
        }
        const error = hostError("AcpStopReason", `The agent ended the turn without a stop reason${stopReason ? ` (${stopReason})` : ""}.`, "engine");
        finish(session, run, { ...failureFrame(error), result: { reason: "failed", threadDisposition: "reusable", ...(usage ? { usage } : {}) } });
      };

      const failRun = (session, run, cause) => {
        if (run.done || session.run !== run) return;
        let error = promptError(session, cause);
        // The prompt's pending request fails with the transport error first; add the sanitized
        // stderr excerpt here, where the run ends, so it is in the run's error and nowhere else.
        if (error.name === "AcpExited" && session.stderrTail.trim() !== "") {
          const excerpt = acpStderrExcerpt(session.stderrTail);
          if (excerpt !== "") error = hostError("AcpExited", `${error.message} ${excerpt}`, "engine");
        }
        const broken = error.kind === "session_lost" || session.broken || session.processGone;
        if (error.kind === "session_lost" || session.processGone) session.broken = true;
        finish(session, run, failureFrame(error, { broken }));
      };

      const trackPrompt = (session, run, promise, generation) => {
        promise.then(
          (result) => {
            if (run.generation === generation) settleRun(session, run, result);
          },
          (cause) => {
            if (run.generation === generation) failRun(session, run, cause);
          },
        );
      };

      /**
       * The session's catalog. Mistral Vibe advertises `thinking` levels for its current model
       * only; keep the levels each model last advertised so a switch away and back does not lose
       * them (T3's applyAcpRegistryLiveConfiguration keeps them per model).
       */
      const catalogFor = (session, setup) => {
        const next = acpModelCatalog(setup, session.agent.id);
        if (session.hooks.modelEfforts !== "current-model") return next;
        const currentId = next.models.find((model) => model.isDefault)?.id;
        const models = next.models.map((model) => {
          if (model.id === currentId || model.efforts.length > 0) return model;
          const known = session.modelEfforts.get(model.id);
          if (known === undefined) return model;
          return {
            ...model,
            efforts: known.efforts.map((effort) => ({ ...effort })),
            ...(known.defaultEffort === undefined ? {} : { defaultEffort: known.defaultEffort }),
          };
        });
        for (const model of models) {
          if (model.efforts.length === 0) continue;
          session.modelEfforts.set(model.id, { efforts: model.efforts.map((effort) => ({ ...effort })), defaultEffort: model.defaultEffort });
        }
        return { models };
      };

      const refreshConfig = (session, configOptions, { preserveMissing = false } = {}) => {
        const normalized = normalizeConfigOptions(configOptions);
        if (normalized === undefined) return;
        // Kiro's model write can answer before its config_option_update advertises effortLevel;
        // keep an option a notification already delivered when the older response omits it.
        session.configOptions = preserveMissing
          ? [...normalized, ...session.configOptions.filter((option) => !normalized.some((candidate) => candidate.id === option.id))]
          : normalized;
        session.models = catalogFor(session, { configOptions: session.configOptions, models: session.modelState });
        session.modes = modeStateOf({ configOptions: session.configOptions }) ?? session.modes;
        for (const [configId, waiters] of session.configWaiters) {
          const option = session.configOptions.find((candidate) => candidate.id === configId);
          if (option === undefined) continue;
          for (const waiter of [...waiters]) waiter(option);
        }
      };

      /** Wait until the agent advertises a config option (Kiro's effortLevel arrives late). */
      const waitForConfigOption = (session, configId, timeoutMs) => {
        const existing = session.configOptions.find((candidate) => candidate.id === configId);
        if (existing !== undefined) return Promise.resolve(existing);
        return new Promise((resolve) => {
          let timer;
          const done = (option) => {
            if (timer !== undefined) clearTimeout(timer);
            const waiters = session.configWaiters.get(configId);
            waiters?.delete(done);
            resolve(option);
          };
          const waiters = session.configWaiters.get(configId) ?? new Set();
          waiters.add(done);
          session.configWaiters.set(configId, waiters);
          timer = setTimeout(() => done(undefined), timeoutMs);
          timer.unref?.();
        });
      };

      const setConfigOption = async (session, configId, value, { allowUnlisted = false } = {}) => {
        const option = session.configOptions.find((candidate) => candidate?.id === configId);
        if (option !== undefined) {
          if (option.type === "boolean" && typeof value !== "boolean") throw hostError("AcpConfigValue", `The agent expects a boolean for ${configId}.`, "invalid_request");
          if (option.type === "select" && typeof value === "string" && !allowUnlisted) {
            const values = collectConfigOptionValues(option);
            if (values.length > 0 && !values.includes(value)) throw hostError("AcpConfigValue", `The agent does not offer ${value} for ${configId}.`, "invalid_request");
          }
        }
        const params = { sessionId: session.engineSessionId, configId, ...(typeof value === "boolean" ? { type: "boolean" } : {}), value };
        const result = await session.connection.request("session/set_config_option", params);
        refreshConfig(session, result?.configOptions, { preserveMissing: isKiro(session) });
        return result;
      };

      const setModel = async (session, modelId) => {
        if (isKiro(session)) {
          // Kiro's own id for "Kiro default" is not known here; a fresh session already runs it.
          if (modelId === "default") return;
          const option = session.configOptions.find((candidate) => candidate.id === KIRO_MODEL_CONFIG_ID) ?? findModelConfigOption(session.configOptions);
          if (option?.type === "select" && option.currentValue === modelId) return;
          // Kiro writes the model even before it advertises the option and for unlisted ids; it
          // fails the next prompt on a model it cannot run (T3's applyKiroModel).
          await setConfigOption(session, option?.id ?? KIRO_MODEL_CONFIG_ID, modelId, { allowUnlisted: true });
          return;
        }
        const option = findModelConfigOption(session.configOptions);
        if (option !== undefined) {
          await setConfigOption(session, option.id, modelId);
          return;
        }
        const available = Array.isArray(session.modelState?.availableModels) ? session.modelState.availableModels : [];
        if (available.length === 0) return; // no catalog to check against: the agent validates
        if (!available.some((model) => model?.modelId === modelId)) {
          throw hostError("AcpModel", "The requested model is not available in this session.", "invalid_request");
        }
        if (session.modelState.currentModelId === modelId) return;
        await session.connection.request("session/set_model", { sessionId: session.engineSessionId, modelId });
        session.modelState = { ...session.modelState, currentModelId: modelId };
        session.models = catalogFor(session, { configOptions: session.configOptions, models: session.modelState });
      };

      const setEffort = async (session, effort) => {
        if (isKiro(session)) {
          // Kiro advertises effortLevel only once the written model runs. Levels the model does
          // not offer are not sent; a missing advert keeps Kiro's own level (T3's applyKiroEffort).
          const option = await waitForConfigOption(session, KIRO_EFFORT_CONFIG_ID, 2_000);
          if (option?.type !== "select" || !collectConfigOptionValues(option).includes(effort)) {
            ctx.log({ type: "kiroEffortSkipped", sessionId: session.id });
            return;
          }
          await setConfigOption(session, KIRO_EFFORT_CONFIG_ID, effort);
          return;
        }
        const option = findThoughtLevelConfigOption(session.configOptions);
        if (option === undefined) return;
        await setConfigOption(session, option.id, effort);
      };

      const setMode = async (session, modeId) => {
        if (isKiro(session)) {
          const option = session.configOptions.find((candidate) => candidate.id === KIRO_AGENT_CONFIG_ID) ??
            session.configOptions.find((candidate) => candidate?.category === "mode" && candidate.type === "select");
          if (option?.type === "select") {
            const values = collectConfigOptionValues(option);
            if (option.currentValue === modeId) return;
            // An agent the session does not list is skipped: Kiro would accept it and not switch.
            if (values.length > 0 && !values.includes(modeId)) {
              ctx.log({ type: "kiroAgentSkipped", sessionId: session.id });
              return;
            }
            await setConfigOption(session, option.id, modeId);
            return;
          }
        }
        const option = Array.isArray(session.configOptions) ? session.configOptions.find((candidate) => candidate?.category === "mode" && candidate.type === "select") : undefined;
        if (option !== undefined) {
          await setConfigOption(session, option.id, modeId);
          return;
        }
        if (session.modes === undefined) return;
        await session.connection.request("session/set_mode", { sessionId: session.engineSessionId, modeId });
        session.modes = { ...session.modes, currentModeId: modeId };
      };

      /**
       * Kiro starts sessions with autopilot "on" (tools run without asking). Policy stays in
       * Rust, so the host writes Kiro's supervised value whenever the option is offered and never
       * writes the other one. An option that is on and offers no supervised value cannot be made
       * safe, so opening the session fails instead of running auto-approved tools.
       */
      const enforceKiroAutopilot = async (session) => {
        if (!isKiro(session) || session.autopilotWrite === true) return;
        const option = session.configOptions.find((candidate) => candidate.id === KIRO_AUTOPILOT_CONFIG_ID);
        if (option?.type !== "select" || option.currentValue === KIRO_AUTOPILOT_OFF) return;
        const values = collectConfigOptionValues(option);
        if (!values.includes(KIRO_AUTOPILOT_OFF)) {
          if (option.currentValue === "on") {
            throw hostError("KiroAutopilot", "Kiro offers autopilot without its supervised value.", "unsupported");
          }
          return;
        }
        session.autopilotWrite = true;
        try {
          await setConfigOption(session, KIRO_AUTOPILOT_CONFIG_ID, KIRO_AUTOPILOT_OFF);
          // The result may not echo configOptions; remember the value the host just wrote so a
          // later advert does not send the same supervised write again.
          session.configOptions = session.configOptions.map((candidate) =>
            candidate.id === KIRO_AUTOPILOT_CONFIG_ID ? { ...candidate, currentValue: KIRO_AUTOPILOT_OFF } : candidate,
          );
        } finally {
          session.autopilotWrite = false;
        }
      };

      const applySelection = async (session, selection = {}) => {
        if (isKiro(session)) {
          // Kiro's agent switch applies that agent's own model, so it runs before the model write;
          // effortLevel waits for the written model's advert (T3's applyKiroModelSelection).
          if (typeof selection.mode === "string" && selection.mode.trim() !== "") await setMode(session, selection.mode.trim());
          if (typeof selection.model === "string" && selection.model.trim() !== "") await setModel(session, selection.model.trim());
          if (typeof selection.effort === "string" && selection.effort.trim() !== "") await setEffort(session, selection.effort.trim());
          return;
        }
        if (typeof selection.model === "string" && selection.model.trim() !== "") await setModel(session, selection.model.trim());
        if (typeof selection.effort === "string" && selection.effort.trim() !== "") await setEffort(session, selection.effort.trim());
        if (typeof selection.mode === "string" && selection.mode.trim() !== "") await setMode(session, selection.mode.trim());
      };

      const validateAgent = (agent) => {
        if (!isRecord(agent) || typeof agent.command !== "string" || agent.command.trim() === "") {
          throw hostError("InvalidAgent", "open.options.agent must name a command.", "invalid_request");
        }
        if (agent.args !== undefined && (!Array.isArray(agent.args) || !agent.args.every((arg) => typeof arg === "string"))) {
          throw hostError("InvalidAgent", "open.options.agent.args must be a list of strings.", "invalid_request");
        }
        if (agent.env !== undefined && !isRecord(agent.env)) {
          throw hostError("InvalidAgent", "open.options.agent.env must be an object.", "invalid_request");
        }
        for (const value of Object.values(agent.env ?? {})) {
          if (typeof value !== "string") throw hostError("InvalidAgent", "open.options.agent.env values must be strings.", "invalid_request");
        }
        return agent;
      };

      const spawnSession = (request, agent) => {
        const { cwd: requestCwd, options = {}, confinement } = request;
        const prefix = confinement?.argvPrefix;
        if (prefix !== undefined && !(Array.isArray(prefix) && prefix.length > 0 && prefix.every((part) => typeof part === "string" && part.length > 0))) {
          throw hostError("InvalidConfinement", "confinement.argvPrefix must be a list of non-empty strings.", "invalid_request");
        }
        const fromCore = confinement?.env !== undefined;
        const env = {};
        for (const [key, value] of Object.entries(fromCore ? confinement.env : process.env)) {
          if (typeof value === "string") env[key] = value;
        }
        for (const [key, value] of Object.entries(agent.env ?? {})) env[key] = value;
        const cwd = confinement?.cwd ?? requestCwd ?? process.cwd();
        const launch = prefix === undefined ? acpLaunch(agent.command, env) : { command: agent.command, args: [] };
        const [command, ...args] = [...(prefix ?? []), launch.command, ...launch.args, ...acpAgentArgs(agent)];
        return { command, args, cwd, env };
      };

      const startSession = async (request, session) => {
        const child = spawn({ command: session.spawn.command, args: session.spawn.args, cwd: session.spawn.cwd, env: session.spawn.env });
        session.child = child;
        session.connection = connect({
          child,
          ctx: { send: ctx.send, log: (event) => ctx.log({ ...event, sessionId: session.id }) },
          onRequest: (message) => onAgentRequest(session, message),
          onNotification: (method, params) => onAgentNotification(session, method, params),
          onCancel: (params) => {
            if (isRecord(params) && typeof params.sessionId === "string" && params.sessionId !== session.engineSessionId) return;
            if (session.run) finish(session, session.run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
          },
          onExit: () => {
            session.processGone = true;
          },
        });
        child.onStderr((chunk) => {
          session.stderrTail = appendStderrTail(session.stderrTail, String(chunk));
        });
        void session.connection.closed.then((exit) => {
          if (session.closing) return;
          session.processGone = true;
          const message = `The agent process exited (${exit?.code ?? exit?.signal ?? "unknown"}).`;
          const excerpt = acpStderrExcerpt(session.stderrTail);
          const error = hostError("AcpExited", excerpt ? `${message} ${excerpt}` : message, "engine");
          ctx.log({ type: "agentExited", sessionId: session.id, code: exit?.code ?? null, signal: exit?.signal ?? null });
          if (session.run) failRun(session, session.run, error);
          else session.broken = true;
        });

        const initialize = await session.connection.request(
          "initialize",
          {
            protocolVersion: ACP_PROTOCOL_VERSION,
            info: { name: "Orbyn", version: "1" },
            capabilities: { elicitation: { form: {} } },
            clientCapabilities: { elicitation: { form: {} } },
            clientInfo: { name: "Orbyn", version: "1" },
          },
          { timeoutMs: startTimeoutMs },
        );
        const normalized = normalizeInitialize(initialize);
        if (normalized.protocolVersion !== 1 && normalized.protocolVersion !== 2) {
          throw hostError("AcpProtocol", "The agent speaks a protocol this app does not know.", "unsupported");
        }
        session.generation = normalized.generation;
        session.caps = normalized.agentCapabilities ?? {};
        session.init = normalized;
      };

      const openSessionSetup = async (session, request, operand) => {
        const cwd = session.spawn.cwd;
        const mcpServers = acpMcpServers(session, request.options?.mcpServers, (event) => ctx.log({ ...event, sessionId: session.id }));
        const setupCall = async () => {
          if (operand === "resume") {
            return session.connection.request("session/load", { sessionId: request.engineSessionId, cwd, mcpServers }, { timeoutMs: requestTimeoutMs });
          }
          if (operand === "fork") {
            return session.connection.request("session/fork", { sessionId: request.engineSessionId, cwd, mcpServers }, { timeoutMs: requestTimeoutMs });
          }
          return session.connection.request("session/new", { cwd, mcpServers }, { timeoutMs: requestTimeoutMs });
        };
        const setupWithAuth = async (call) => {
          try {
            return await call();
          } catch (cause) {
            const error = acpError(cause);
            if (error.kind !== "auth") throw error;
            const methodId = text(request.options?.authMethodId);
            if (methodId === undefined) throw hostError("AcpAuthRequired", "Sign in to the agent before opening a session.", "auth");
            const advertised = Array.isArray(session.init?.authMethods) ? session.init.authMethods : [];
            if (advertised.length > 0 && !advertised.some((method) => method?.id === methodId)) {
              throw hostError("AcpAuthMethod", "The agent did not advertise that sign-in method.", "auth");
            }
            await session.connection.request("authenticate", { methodId }, { timeoutMs: requestTimeoutMs });
            return await call();
          }
        };
        const adoptSetup = (result, engineSessionId) => {
          session.engineSessionId = engineSessionId;
          session.modelState = isRecord(result?.models) ? result.models : null;
          session.configOptions = normalizeConfigOptions(result?.configOptions) ?? [];
          session.modes = modeStateOf(result);
          session.models = catalogFor(session, { configOptions: session.configOptions, models: session.modelState });
        };
        let result;
        if (operand === "fork" && isKiro(session)) {
          // Kiro can only fork a session it has started in this process, and a turn fork names
          // the source prompt's `userMessageId`. T3 starts a session, configures it, and then
          // calls session/fork with `_meta.kiro.messageId` (packages/provider-kiro adapter.ts).
          const provisional = await setupWithAuth(() => session.connection.request("session/new", { cwd, mcpServers }, { timeoutMs: requestTimeoutMs }));
          const provisionalId = text(provisional?.sessionId);
          if (provisionalId === undefined) throw hostError("AcpSession", "The agent returned no session id.", "engine");
          adoptSetup(provisional, provisionalId);
          await applySelection(session, request.options ?? {});
          await enforceKiroAutopilot(session);
          const atTurn = text(request.options?.atTurn) ?? text(request.options?.forkAtTurn) ?? text(request.atTurn);
          result = await setupWithAuth(() =>
            session.connection.request(
              "session/fork",
              {
                sessionId: request.engineSessionId,
                cwd,
                mcpServers,
                ...(atTurn === undefined ? {} : { _meta: { kiro: { messageId: atTurn } } }),
              },
              { timeoutMs: requestTimeoutMs },
            ),
          );
        } else {
          result = await setupWithAuth(setupCall);
        }
        if (operand === "create" || operand === "fork") {
          const engineSessionId = text(result?.sessionId);
          if (engineSessionId === undefined) throw hostError("AcpSession", "The agent returned no session id.", "engine");
          adoptSetup(result, engineSessionId);
        } else {
          adoptSetup(result, request.engineSessionId);
        }
      };

      const onAgentNotification = (session, method, params) => {
        if (method !== "session/update") {
          ctx.log({ type: "unknownAgentNotification", method });
          return;
        }
        const paramsSession = params.sessionId;
        if (typeof paramsSession === "string" && paramsSession !== session.engineSessionId) return;
        const update = params.update;
        if (!wellFormedUpdate(update)) {
          ctx.log({ type: "malformedSessionUpdate", method: "session/update" });
          return;
        }
        if (update.sessionUpdate === "config_option_update") {
          refreshConfig(session, update.configOptions);
          if (isKiro(session)) {
            // Kiro can advertise a new option set at any time; a supervised session never keeps
            // autopilot on, so correct an advert that says on (without delaying the delta stream).
            void enforceKiroAutopilot(session).catch((cause) => {
              ctx.log({ type: "kiroAutopilotFailed", sessionId: session.id, kind: acpError(cause).kind });
            });
          }
        }
        if (update.sessionUpdate === "current_mode_update" && session.modes !== undefined) {
          const modeId = text(update.currentModeId);
          if (modeId !== undefined) session.modes = { ...session.modes, currentModeId: modeId };
        }
        if (update.sessionUpdate === "tool_call" || update.sessionUpdate === "tool_call_update") trackTool(session, update);
        const run = session.run;
        if (!run || run.done) {
          ctx.log({ type: "updateWithoutRun", sessionId: session.id, sessionUpdate: String(update.sessionUpdate) });
          return;
        }
        emit(run, { type: "delta", sendId: run.sendId, update });
      };

      const trackTool = (session, update) => {
        const id = update.toolCallId;
        const previous = session.tools.get(id) ?? { toolCallId: id };
        const merged = { ...previous };
        for (const key of ["title", "kind", "status", "content", "locations", "rawInput", "rawOutput", "_meta"]) {
          if (update[key] !== undefined) merged[key] = update[key];
        }
        session.tools.set(id, merged);
        if (session.tools.size > MAX_TRACKED_TOOL_CALLS) session.tools.delete(session.tools.keys().next().value);
      };

      const withTrackedToolCall = (session, toolCall) => {
        if (!isRecord(toolCall)) return toolCall;
        const tracked = typeof toolCall.toolCallId === "string" ? session.tools.get(toolCall.toolCallId) : undefined;
        if (tracked === undefined) return toolCall;
        const merged = { ...tracked, ...toolCall };
        if (toolCall.title === undefined && tracked.title !== undefined) merged.title = tracked.title;
        if (toolCall.kind === undefined && tracked.kind !== undefined) merged.kind = tracked.kind;
        if (toolCall.rawInput === undefined && tracked.rawInput !== undefined) merged.rawInput = tracked.rawInput;
        return session.hooks.normalizeToolCall?.(merged) ?? merged;
      };

      /**
       * ACP v1 names the tool call `toolCall`; v2 names it `subject` (`{type:"tool_call",
       * toolCall}` or a command). T3 normalizes both to a tool call before the ask reaches its UI.
       */
      const requestToolCall = (params, message) => {
        if (isRecord(params.toolCall)) return params.toolCall;
        const subject = isRecord(params.subject) ? params.subject : undefined;
        if (subject?.type === "tool_call" && isRecord(subject.toolCall)) return subject.toolCall;
        return {
          toolCallId: typeof subject?.toolCallId === "string" ? subject.toolCallId : String(message.id),
          ...(typeof params.title === "string" ? { title: params.title } : {}),
          kind: subject?.type === "command" ? "execute" : "other",
        };
      };

      const permissionTool = (toolCall) => text(toolCall?.title) ?? text(toolCall?.kind) ?? "tool";

      const publishPermission = (session, message) => {
        const params = isRecord(message.params) ? message.params : {};
        if (typeof params.sessionId === "string" && params.sessionId !== session.engineSessionId) {
          try {
            session.connection.respond(message.id, { outcome: { outcome: "cancelled" } });
          } catch {
            // The process is gone; the ask is unanswerable either way.
          }
          return;
        }
        if (!session.run || session.run.done) {
          // An ask with no run has no `sendId` to route; policy cannot answer it.
          ctx.log({ type: "askWithoutRun", method: "session/request_permission" });
          try {
            session.connection.respond(message.id, { outcome: { outcome: "cancelled" } });
          } catch {
            // ignore
          }
          return;
        }
        const toolCall = withTrackedToolCall(session, requestToolCall(params, message));
        const entry = {
          kind: "permission",
          session,
          run: session.run,
          jsonrpcId: message.id,
          requestId: `p-${++counter}`,
          params,
        };
        pending.set(entry.requestId, entry);
        forSession(session, {
          type: "permission",
          ...sendIdOf(session),
          requestId: entry.requestId,
          tool: permissionTool(toolCall),
          input: { ...params, toolCall },
          options: Array.isArray(params.options) ? params.options : [],
        });
      };

      const publishElicitation = (session, message) => {
        const params = isRecord(message.params) ? message.params : {};
        if (params.mode !== "form" || !isRecord(params.requestedSchema)) {
          try {
            session.connection.respond(message.id, { action: "decline" });
          } catch {
            // ignore
          }
          return;
        }
        if (typeof params.sessionId === "string" && params.sessionId !== session.engineSessionId) {
          try {
            session.connection.respond(message.id, { action: "decline" });
          } catch {
            // ignore
          }
          return;
        }
        const native = session.hooks.extractElicitationForm?.(params);
        const form = native ?? (() => {
          const parsed = parseElicitationForm(params.requestedSchema, typeof params.message === "string" ? params.message : "The agent asks a question.", {
            allowOther: isRecord(params._meta) && params._meta["cognition.ai/allowOther"] === true,
          });
          if (parsed === undefined) return undefined;
          return {
            questions: parsed.questions,
            respond: (answers) => {
              const content = parsed.content(answers);
              // A required field without a valid answer keeps the question open for the desk.
              if (content === undefined) throw hostError("InvalidAnswer", "Answer every required question.", "invalid_request");
              return { action: "accept", content };
            },
          };
        })();
        if (form === undefined) {
          try {
            session.connection.respond(message.id, { action: "decline" });
          } catch {
            // ignore
          }
          return;
        }
        if (!session.run || session.run.done) {
          ctx.log({ type: "askWithoutRun", method: "elicitation/create" });
          try {
            session.connection.respond(message.id, { action: "cancel" });
          } catch {
            // ignore
          }
          return;
        }
        const entry = { kind: "question", session, run: session.run, jsonrpcId: message.id, requestId: `q-${++counter}`, form };
        pending.set(entry.requestId, entry);
        forSession(session, {
          type: "question",
          ...sendIdOf(session),
          requestId: entry.requestId,
          tool: "AskUserQuestion",
          input: { questions: form.questions },
        });
      };

      const onAgentRequest = (session, message) => {
        if (message.method === "session/request_permission") return publishPermission(session, message);
        if (message.method === "elicitation/create" || message.method === "session/elicitation" || message.method === "unstable_session/elicitation") {
          return publishElicitation(session, message);
        }
        // A host that advertises no fs and no terminal refuses them; policy is not the agent's.
        ctx.log({ type: "unsupportedAgentRequest", method: message.method });
        try {
          session.connection.respondError(message.id, -32601, `Method not found: ${message.method}`);
        } catch {
          // The process is gone.
        }
      };

      const messageText = (message) => {
        if (typeof message === "string") return message;
        if (isRecord(message) && typeof message.text === "string") return message.text;
        throw hostError("InvalidMessage", "message must be text.", "invalid_request");
      };

      const attachmentList = (message) => {
        if (!isRecord(message) || !Array.isArray(message.attachments)) return [];
        return message.attachments.flatMap((entry) => {
          if (typeof entry === "string") return [{ path: entry }];
          if (isRecord(entry) && typeof entry.path === "string") return [{ path: entry.path, mimeType: typeof entry.mimeType === "string" ? entry.mimeType : undefined }];
          return [];
        });
      };

      const promptBlocks = async (session, textValue, message) => {
        const blocks = [{ type: "text", text: textValue }];
        const promptCapabilities = session.caps?.promptCapabilities ?? {};
        for (const attachment of attachmentList(message)) {
          let bytes;
          try {
            bytes = await readFile(attachment.path);
          } catch (cause) {
            throw hostError("AcpAttachment", `Could not read attachment ${attachment.path}.`, "invalid_request", cause);
          }
          const mimeType = attachment.mimeType ?? (attachment.path.toLowerCase().endsWith(".png") ? "image/png" : attachment.path.toLowerCase().endsWith(".jpg") || attachment.path.toLowerCase().endsWith(".jpeg") ? "image/jpeg" : undefined);
          if (mimeType?.startsWith("image/")) {
            if (promptCapabilities.image !== true) throw hostError("AcpAttachment", "The agent does not accept image prompts.", "unsupported");
            blocks.push({ type: "image", data: bytes.toString("base64"), mimeType });
            continue;
          }
          if (promptCapabilities.embeddedContext === true) {
            blocks.push({
              type: "resource",
              resource: {
                uri: new URL(`file://${attachment.path.startsWith("/") ? "" : "/"}${attachment.path}`).href,
                ...(mimeType ? { mimeType } : {}),
                text: bytes.toString("utf8"),
              },
            });
          } else {
            blocks[0].text += `\n\n[attachment: ${attachment.path}]`;
          }
        }
        return blocks;
      };

      const open = async (request) => {
        if (sessions.has(request.sessionId)) throw hostError("SessionOpen", "this session is already open", "invalid_request");
        const operation = request.operation;
        if (operation !== "create" && operation !== "resume" && operation !== "fork") {
          throw hostError("InvalidOpen", "open.operation must be create, resume or fork.", "invalid_request");
        }
        const agent = validateAgent(request.options?.agent);
        if (operation !== "create" && (typeof request.engineSessionId !== "string" || request.engineSessionId.length === 0)) {
          throw hostError("InvalidOpen", `${operation} needs engineSessionId.`, "invalid_request");
        }
        const spawn = spawnSession(request, agent);
        const session = {
          id: request.sessionId,
          cwd: spawn.cwd,
          options: request.options ?? {},
          agent,
          hooks: acpAgentHooks(agent.id),
          spawn,
          child: undefined,
          connection: undefined,
          engineSessionId: undefined,
          init: undefined,
          caps: {},
          generation: 1,
          modelState: null,
          configOptions: [],
          configWaiters: new Map(),
          autopilotWrite: false,
          modelEfforts: new Map(),
          models: { models: [] },
          modes: undefined,
          run: null,
          tools: new Map(),
          stderrTail: "",
          closing: false,
          broken: false,
          processGone: false,
        };
        try {
          await startSession(request, session);
          if (operation === "resume" && session.caps?.loadSession !== true) {
            throw hostError("AcpResume", "The agent cannot load a saved session.", "unsupported");
          }
          if (operation === "fork" && session.caps?.sessionCapabilities?.fork == null) {
            throw hostError("AcpFork", "The agent cannot fork a session.", "unsupported");
          }
          await openSessionSetup(session, request, operation);
          await applySelection(session, { model: request.options?.model, effort: request.options?.effort, mode: request.options?.mode });
          await enforceKiroAutopilot(session);
        } catch (cause) {
          session.closing = true;
          session.connection?.failAll(acpError(cause));
          await Promise.resolve(session.child?.close()).catch(() => {});
          throw acpError(cause);
        }
        sessions.set(session.id, session);
        return { engineSessionId: session.engineSessionId };
      };

      const beginRun = (session, request) => {
        const run = {
          sendId: request.id,
          open: false,
          buffer: [],
          done: false,
          generation: 0,
          prompt: undefined,
          compact: false,
          settledResolve: undefined,
          settled: undefined,
        };
        run.settled = new Promise((resolve) => (run.settledResolve = resolve));
        session.run = run;
        return run;
      };
      const openRun = (run) => {
        setImmediate(() => {
          run.open = true;
          for (const frame of run.buffer.splice(0)) ctx.send(frame);
        });
      };
      const acceptedRun = (session, request) => ({
        runId: `run-${request.id}`,
        engineSessionId: session.engineSessionId,
        ...(request.messageId === undefined ? {} : { messageId: request.messageId }),
      });

      /**
       * Kiro's `/compact` is its `_kiro/session/compact` extension, not a prompt: T3 answers a
       * `/compact` turn from that request (packages/provider-kiro/src/server/adapter.ts). It runs
       * as one completed run with no prompt and is used only when Kiro advertised the method;
       * otherwise `/compact` travels as an ordinary message.
       */
      const sendKiroCompact = async (session, request) => {
        const run = beginRun(session, request);
        run.compact = true;
        let promise;
        try {
          promise = session.connection.request(KIRO_COMPACT_METHOD, { sessionId: session.engineSessionId }, { timeoutMs: 0 });
        } catch (cause) {
          if (session.run === run) session.run = null;
          throw acpError(cause);
        }
        run.prompt = promise;
        promise.then(
          (result) => {
            if (run.done) return;
            if (result?.success !== true) {
              failRun(session, run, hostError("KiroCompact", "Kiro could not compact the conversation.", "engine"));
              return;
            }
            finish(session, run, { ok: true, result: { reason: "completed", threadDisposition: "reusable" } });
          },
          (cause) => failRun(session, run, cause),
        );
        openRun(run);
        return acceptedRun(session, request);
      };

      const send = async (request) => {
        const session = sessionOf(request.sessionId);
        if (session.broken || session.processGone) throw hostError("AcpSessionLost", "The agent lost this conversation.", "session_lost");
        if (session.run) throw hostError("RunInProgress", "a run is already open on this session", "invalid_request");
        const textValue = messageText(request.message);
        if (
          isKiro(session) &&
          isKiroCompactCommand(textValue) &&
          attachmentList(request.message).length === 0 &&
          kiroAdvertises(session, KIRO_COMPACT_METHOD)
        ) {
          await enforceKiroAutopilot(session);
          return sendKiroCompact(session, request);
        }
        await applySelection(session, request.options ?? {});
        await enforceKiroAutopilot(session);
        const blocks = await promptBlocks(session, textValue, request.message);
        const run = beginRun(session, request);
        let promise;
        try {
          run.generation += 1;
          const generation = run.generation;
          promise = session.connection.request("session/prompt", { sessionId: session.engineSessionId, prompt: blocks }, { timeoutMs: 0 });
          run.prompt = promise;
          trackPrompt(session, run, promise, generation);
        } catch (cause) {
          if (session.run === run) session.run = null;
          throw acpError(cause);
        }
        openRun(run);
        return acceptedRun(session, request);
      };

      const steer = async (request) => {
        const session = runOf(request.sendId);
        if (!session || session.broken || session.run?.done) return { outcome: "rejected", messageId: request.messageId };
        if (typeof request.text !== "string") throw hostError("InvalidMessage", "steer.text must be text.", "invalid_request");
        const run = session.run;
        if (isKiro(session) && run.compact !== true) {
          // Kiro's native steer queues the message for the running turn; it waits for the tool
          // in flight and never cancels it (T3's KiroSteering). The prompt's own reply ends the run.
          try {
            const result = await session.connection.request(
              KIRO_STEER_METHOD,
              { sessionId: session.engineSessionId, message: request.text },
              { timeoutMs: requestTimeoutMs },
            );
            if (result?.queued !== true) {
              ctx.log({ type: "steerRejected", kind: "engine" });
              return { outcome: "rejected", messageId: request.messageId };
            }
            return { outcome: "queued", messageId: request.messageId };
          } catch (cause) {
            ctx.log({ type: "steerRejected", kind: acpError(cause).kind });
            return { outcome: "rejected", messageId: request.messageId };
          }
        }
        refuseAsks(session, run);
        const previousGeneration = run.generation;
        run.generation += 1;
        const generation = run.generation;
        try {
          session.connection.notify("session/cancel", { sessionId: session.engineSessionId });
        } catch (cause) {
          run.generation = previousGeneration;
          ctx.log({ type: "steerCancelFailed", kind: acpError(cause).kind });
          return { outcome: "rejected", messageId: request.messageId };
        }
        try {
          const blocks = await promptBlocks(session, request.text, undefined);
          const promise = session.connection.request("session/prompt", { sessionId: session.engineSessionId, prompt: blocks }, { timeoutMs: 0 });
          run.prompt = promise;
          trackPrompt(session, run, promise, generation);
          return { outcome: "accepted", messageId: request.messageId };
        } catch (cause) {
          // The old prompt is still the run's (the cancel that went out ends it as cancelled).
          run.generation = previousGeneration;
          ctx.log({ type: "steerRejected", kind: acpError(cause).kind });
          return { outcome: "rejected", messageId: request.messageId };
        }
      };

      const cancel = async (request) => {
        const session = runOf(request.sendId);
        if (!session) return { cancelled: false };
        const run = session.run;
        if (!run || run.done) return { cancelled: false };
        refuseAsks(session, run);
        try {
          session.connection.notify("session/cancel", { sessionId: session.engineSessionId });
        } catch (cause) {
          ctx.log({ type: "cancelFailed", kind: acpError(cause).kind });
        }
        await Promise.race([run.settled, new Promise((resolve) => setTimeout(resolve, cancelGraceMs).unref?.())]);
        finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
        return { cancelled: true };
      };

      const respond = async (request) => {
        const entry = pending.get(request.requestId);
        if (!entry) {
          ctx.log({ type: "lateRespond", requestId: request.requestId });
          return null;
        }
        const decision = isRecord(request.decision) ? request.decision : {};
        if (entry.kind === "permission") {
          const options = Array.isArray(entry.params.options) ? entry.params.options : [];
          const allow = options.find((option) => option?.kind === "allow_once");
          const reject = options.find((option) => option?.kind === "reject_once");
          const selected = decision.behavior === "allow" && allow ? allow : reject;
          answerAsk(entry, selected === undefined ? { outcome: { outcome: "cancelled" } } : { outcome: { outcome: "selected", optionId: selected.optionId } });
          return null;
        }
        if (decision.behavior === "deny" || !isRecord(decision.answers)) {
          answerAsk(entry, { action: "cancel" });
          return null;
        }
        const answers = formAnswers(entry.form, decision);
        let result;
        try {
          result = entry.form.respond(answers);
        } catch (cause) {
          throw acpError(cause, "invalid_request");
        }
        answerAsk(entry, result);
        return null;
      };

      const closeSession = async (request) => {
        const session = sessions.get(request.sessionId);
        if (!session) return null;
        session.closing = true;
        const run = session.run;
        if (run) {
          refuseAsks(session, run);
          try {
            session.connection.notify("session/cancel", { sessionId: session.engineSessionId });
          } catch {
            // ignore
          }
          await Promise.race([run.settled, new Promise((resolve) => setTimeout(resolve, cancelGraceMs).unref?.())]);
          finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
        }
        refuseAsks(session);
        if (session.engineSessionId !== undefined && session.caps?.sessionCapabilities?.close != null) {
          await session.connection.request("session/close", { sessionId: session.engineSessionId }, { timeoutMs: CLOSE_REQUEST_TIMEOUT_MS }).catch(() => {});
        }
        sessions.delete(request.sessionId);
        session.connection.failAll(hostError("AcpClosed", "The session is closed.", "engine"));
        await Promise.resolve(session.child?.close()).catch(() => {});
        return null;
      };

      const models = async (request) => {
        if (request.sessionId !== undefined) return sessionOf(request.sessionId).models;
        if (request.agent === undefined) throw hostError("InvalidModels", "models needs an agent or an open session.", "invalid_request");
        const agent = validateAgent(request.agent);
        const spawn = spawnSession({ cwd: request.cwd ?? process.cwd(), options: {}, confinement: request.confinement }, agent);
        const session = {
          id: `models-${request.id}`,
          cwd: spawn.cwd,
          options: {},
          agent,
          hooks: acpAgentHooks(agent.id),
          spawn,
          child: undefined,
          connection: undefined,
          engineSessionId: undefined,
          init: undefined,
          caps: {},
          generation: 1,
          modelState: null,
          configOptions: [],
          configWaiters: new Map(),
          autopilotWrite: false,
          modelEfforts: new Map(),
          modes: undefined,
          run: null,
          tools: new Map(),
          stderrTail: "",
          closing: false,
          broken: false,
          processGone: false,
        };
        try {
          await startSession(request, session);
          const result = await session.connection.request("session/new", { cwd: session.spawn.cwd, mcpServers: [] }, { timeoutMs: requestTimeoutMs });
          session.engineSessionId = text(result?.sessionId);
          return acpModelCatalog({ configOptions: result?.configOptions, models: result?.models }, agent.id);
        } catch (cause) {
          throw acpError(cause);
        } finally {
          session.closing = true;
          if (session.engineSessionId !== undefined && session.caps?.sessionCapabilities?.close != null) {
            await session.connection?.request("session/close", { sessionId: session.engineSessionId }, { timeoutMs: CLOSE_REQUEST_TIMEOUT_MS }).catch(() => {});
          }
          session.connection?.failAll(hostError("AcpClosed", "The session is closed.", "engine"));
          await Promise.resolve(session.child?.close()).catch(() => {});
        }
      };

      return {
        async handle(request) {
          switch (request.type) {
            case "open":
              return open(request);
            case "send":
              return send(request);
            case "steer":
              return steer(request);
            case "cancel":
              return cancel(request);
            case "respond":
              return respond(request);
            case "closeSession":
              return closeSession(request);
            case "models":
              return models(request);
            default:
              throw hostError("UnsupportedRequest", `unsupported request ${request.type}`, "unsupported");
          }
        },
        async close() {
          for (const session of sessions.values()) {
            session.closing = true;
            session.connection?.failAll(hostError("AcpClosed", "The host is closing.", "engine"));
            if (session.run && !session.run.done) {
              const run = session.run;
              refuseAsks(session, run);
              finish(session, run, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
            }
            refuseAsks(session);
            await Promise.resolve(session.child?.close()).catch(() => {});
          }
          sessions.clear();
        },
      };
    },
  };
}
