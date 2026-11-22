// Per-agent quirks for the generic ACP family. The table is the mechanism T3 Code keeps in
// AcpAdapterV2Flavor (MIT, Copyright (c) 2026 T3 Tools Inc., commit
// 9168ca73d02003557725430b515b44e1551544af): each hook sits exactly where T3 applies it.
//
// Hooks:
// - `extractElicitationForm(request)`: T3's flavor.extractElicitationForm. Return
//   `{questions, respond(answers)}` to replace the generic elicitation form parser, or undefined
//   to use it. `questions` is the host protocol's question list; `respond` returns the ACP
//   `elicitation/create` result (`{action:"accept"|"decline"|"cancel", content?}`).
// - `normalizeToolCall(toolCall)`: T3's flavor.normalizeToolCall, applied to the tool call before
//   a permission frame names it (some agents leave the title or kind off and name it elsewhere).
//
// Plain fields are the agent's config and catalog rules, read by acp.mjs:
// - `launchArgs`: args used when the core gives this id an empty argument list (Kiro's V3 launch).
// - `configIds`: exact ACP config ids for the host's model, effort, mode and autopilot selections.
// - `autopilotOff`: Kiro's supervised value; the host writes it when the offered value is not
//   already supervised and never writes the other value.
// - `compactMethod` / `steerMethod`: Kiro extension methods the host uses for `/compact` and a
//   native steer.
// - `modelEfforts: "current-model"`: the session's reasoning option describes its current model
//   only, so other models keep no session levels (Mistral Vibe's `thinking` category).
// - `bareModelNames`: the agent's model ids are their names when it sends no name (GLM, Kimi).
// - `modelOptionArrivesLate`: the model config option appears after `session/new` (Kiro).
// - `allowUnlistedModel`: write the model the core selected even when it is not listed (Kiro).
//
// The `default` entry has no quirks, so the generic parser, the agent's own tool naming and the
// generic model catalog apply.

const isRecord = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
const trimmed = (value) => (typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined);

// ---------------------------------------------------------------------------------------------
// Droid: factory-droid pairs each choice with an optional *_other field and sends the form as
// question_N / question_N_other. T3 hides the companion (the client already offers custom
// answers) and restores it under the choice "other" in the response.
// (packages/provider-acp-registry/src/server/droidAcp.ts, MIT, T3 Code.)
// ---------------------------------------------------------------------------------------------

const DROID_QUESTION = /^question_[1-9]\d*$/;
const DROID_OPTION = /^(?:option_\d+|other)$/;
const DROID_ATTACHMENT = /^\n\nAttached (?:image|file) "(?:\\.|[^"\\\r\n])*": "/;

function isDroidForm(requestedSchema) {
  return isRecord(requestedSchema) && isRecord(requestedSchema.properties) && Array.isArray(requestedSchema.required) && requestedSchema.required.every((id) => typeof id === "string");
}

function isDroidQuestion(field) {
  if (!isRecord(field) || field.type !== "string" || trimmed(field.title) === undefined || trimmed(field.description) === undefined || !Array.isArray(field.oneOf)) return false;
  return field.oneOf.every(
    (option) => isRecord(option) && typeof option.const === "string" && DROID_OPTION.test(option.const) && trimmed(option.title) !== undefined,
  );
}

function isDroidCompanion(field) {
  return isRecord(field) && field.type === "string";
}

export function extractDroidElicitationForm(request) {
  if (request?.mode !== "form" || !isDroidForm(request.requestedSchema)) return undefined;
  const { properties, required } = request.requestedSchema;
  const questions = [];
  for (const [id, field] of Object.entries(properties)) {
    if (!DROID_QUESTION.test(id)) continue;
    if (!isDroidQuestion(field) || !isDroidCompanion(properties[`${id}_other`])) return undefined;
    const options = field.oneOf
      .filter((option) => option.const !== "other")
      .map((option) => ({ label: trimmed(option.title), description: trimmed(option.title), value: option.const }));
    if (options.length === 0 || !field.oneOf.some((option) => option.const === "other")) return undefined;
    questions.push({ id, header: trimmed(field.title), question: trimmed(field.description), options });
  }
  // Recognize only complete forms; leave every other schema to the ACP fallback.
  const requiredIds = new Set(required);
  if (
    questions.length === 0 ||
    Object.keys(properties).length !== questions.length * 2 ||
    requiredIds.size !== questions.length ||
    questions.some((question) => !requiredIds.has(question.id))
  ) {
    return undefined;
  }
  return {
    questions,
    respond: (answers) => {
      const entries = [];
      for (const question of questions) {
        const answer = Object.hasOwn(answers, question.id) ? answers[question.id] : undefined;
        if (typeof answer !== "string" || answer.trim().length === 0) return { action: "cancel" };
        if (question.options.some((option) => option.value === answer)) {
          entries.push([question.id, answer]);
        } else {
          // Intake appends server attachment references to the selected choice id. Keep them with
          // the custom text, and turn a selected option back into its label (T3's rule).
          const attachmentStart = answer.indexOf("\n\n");
          const selectedOption = question.options.find(
            (option) => option.value === answer.slice(0, attachmentStart) && DROID_ATTACHMENT.test(answer.slice(attachmentStart)),
          );
          const text = selectedOption ? `${selectedOption.label}${answer.slice(attachmentStart)}` : answer;
          entries.push([question.id, "other"], [`${question.id}_other`, text]);
        }
      }
      return { action: "accept", content: Object.fromEntries(entries) };
    },
  };
}

// ---------------------------------------------------------------------------------------------
// OMP: askDialog pairs qN choices with qN__other, and puts the question in the title.
// (packages/provider-acp-registry/src/server/ompAcp.ts, MIT, T3 Code.)
// ---------------------------------------------------------------------------------------------

const OMP_QUESTION = /^q\d+$/;

function isOmpForm(requestedSchema) {
  return isRecord(requestedSchema) && isRecord(requestedSchema.properties);
}

function isOmpChoice(choice) {
  return isRecord(choice) && trimmed(choice.const) !== undefined && trimmed(choice.title) !== undefined && (choice.description === undefined || typeof choice.description === "string");
}

function isOmpQuestion(field) {
  return (
    isRecord(field) &&
    trimmed(field.title) !== undefined &&
    (field.description === undefined || typeof field.description === "string") &&
    field.type === "string" &&
    Array.isArray(field.oneOf) &&
    field.oneOf.every(isOmpChoice)
  );
}

function isOmpCompanion(field) {
  return isRecord(field) && field.type === "string" && field.title === "Other (type your own)";
}

export function extractOmpElicitationForm(request) {
  if (request?.mode !== "form" || !isOmpForm(request.requestedSchema)) return undefined;
  const { properties } = request.requestedSchema;
  const questions = [];
  for (const [id, field] of Object.entries(properties)) {
    if (!OMP_QUESTION.test(id)) continue;
    if (!isOmpQuestion(field) || !isOmpCompanion(properties[`${id}__other`])) return undefined;
    const choices = field.oneOf;
    if (choices.length === 0) return undefined;
    questions.push({
      id,
      header: trimmed(field.description) ?? `Question ${questions.length + 1}`,
      question: trimmed(field.title),
      options: choices.map((option) => ({
        label: trimmed(option.title),
        value: option.const,
        description: trimmed(option.description) ?? trimmed(option.title),
      })),
    });
  }
  // Do not hide unknown fields or partially reinterpret a different form.
  if (questions.length === 0 || Object.keys(properties).length !== questions.length * 2) return undefined;
  return {
    questions,
    respond: (answers) => {
      const content = {};
      for (const question of questions) {
        const answer = Object.hasOwn(answers, question.id) ? answers[question.id] : undefined;
        if (typeof answer !== "string" || answer.trim().length === 0) return { action: "cancel" };
        if (question.options.some((option) => option.value === answer)) content[question.id] = answer;
        else content[`${question.id}__other`] = answer;
      }
      return { action: "accept", content };
    },
  };
}

// ---------------------------------------------------------------------------------------------
// Kimi: tool titles are the bare tool name with the streamed JSON arguments appended, e.g.
// `delegate_task: {"mode":"async","task":"Ru` (the recording may stop mid-string). A permission
// must name the tool, so drop the argument suffix; a built-in's prose title is left alone.
// (packages/provider-acp/src/server/runtimeModel.ts T3_MCP_BARE_TITLE_CALL, MIT, T3 Code.)
// ---------------------------------------------------------------------------------------------

const KIMI_STREAMED_ARGUMENTS = /^([A-Za-z0-9_]+):\s*\{/;

export function normalizeKimiToolCall(toolCall) {
  if (!isRecord(toolCall) || typeof toolCall.title !== "string") return toolCall;
  const match = KIMI_STREAMED_ARGUMENTS.exec(toolCall.title.trim());
  return match === null ? toolCall : { ...toolCall, title: match[1] };
}

// ---------------------------------------------------------------------------------------------
// Kiro: launch and session config. T3's provider-kiro (acpSupport.ts/adapter.ts, MIT, T3 Code).
// ---------------------------------------------------------------------------------------------

/** Kiro's V3 ACP launch; the V2 engine rejects these flags and mode/model are per session. */
export const KIRO_ACP_ARGS = Object.freeze(["acp", "--agent-engine=v3", "--auth-method=cli"]);

export const KIRO_MODEL_CONFIG_ID = "model";
export const KIRO_EFFORT_CONFIG_ID = "effortLevel";
export const KIRO_AGENT_CONFIG_ID = "mode";
export const KIRO_AUTOPILOT_CONFIG_ID = "autopilot";
/** Kiro's Supervised value: every tool prompt reaches the host's policy, never auto-approved. */
export const KIRO_AUTOPILOT_OFF = "off";
/** Kiro's compaction extension, advertised in agentCapabilities._meta.kiro.extensionMethods. */
export const KIRO_COMPACT_METHOD = "_kiro/session/compact";
/** Kiro's native steer extension: queues the message for the running turn. */
export const KIRO_STEER_METHOD = "_session/steer";
export const KIRO_CONFIG_IDS = Object.freeze({
  model: KIRO_MODEL_CONFIG_ID,
  effort: KIRO_EFFORT_CONFIG_ID,
  mode: KIRO_AGENT_CONFIG_ID,
  autopilot: KIRO_AUTOPILOT_CONFIG_ID,
});

export const ACP_AGENTS = Object.freeze({
  default: Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: undefined,
  }),
  droid: Object.freeze({
    extractElicitationForm: extractDroidElicitationForm,
    normalizeToolCall: undefined,
  }),
  omp: Object.freeze({
    extractElicitationForm: extractOmpElicitationForm,
    normalizeToolCall: undefined,
  }),
  kimi: Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: normalizeKimiToolCall,
    bareModelNames: true,
  }),
  kiro: Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: undefined,
    launchArgs: KIRO_ACP_ARGS,
    configIds: KIRO_CONFIG_IDS,
    autopilotOff: KIRO_AUTOPILOT_OFF,
    compactMethod: KIRO_COMPACT_METHOD,
    steerMethod: KIRO_STEER_METHOD,
    modelOptionArrivesLate: true,
    allowUnlistedModel: true,
  }),
  "mistral-vibe": Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: undefined,
    modelEfforts: "current-model",
  }),
  glm: Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: undefined,
    bareModelNames: true,
  }),
});

/** The hooks for an agent id; unknown ids get the default behaviour. */
export function acpAgentHooks(id) {
  if (typeof id !== "string" || id.trim() === "") return ACP_AGENTS.default;
  return ACP_AGENTS[id] ?? ACP_AGENTS.default;
}
