// Per-agent quirks for the generic ACP family. The table is the mechanism T3 Code keeps in
// AcpAdapterV2Flavor (MIT, Copyright (c) 2026 T3 Tools Inc., commit
// 9168ca73d02003557725430b515b44e1551544af): each hook sits exactly where T3 applies it, so a
// later task can add droid, omp, kimi, kiro, vibe and the rest without moving any call site. This
// task ships the table and the `default` entry only.
//
// Hooks:
// - `extractElicitationForm(request)`: T3's flavor.extractElicitationForm. Return
//   `{questions, respond(answers)}` to replace the generic elicitation form parser, or undefined
//   to use it. `questions` is the host protocol's question list; `respond` returns the ACP
//   `elicitation/create` result (`{action:"accept"|"decline"|"cancel", content?}`).
// - `normalizeToolCall(toolCall)`: T3's flavor.normalizeToolCall, applied to the tool call before
//   a permission frame names it (some agents leave the title or kind off and name it elsewhere).
//
// The default entry has no quirks, so the generic parser and the agent's own tool naming apply.

export const ACP_AGENTS = Object.freeze({
  default: Object.freeze({
    extractElicitationForm: undefined,
    normalizeToolCall: undefined,
  }),
});

/** The hooks for an agent id; unknown ids get the default behaviour. */
export function acpAgentHooks(id) {
  if (typeof id !== "string" || id.trim() === "") return ACP_AGENTS.default;
  return ACP_AGENTS[id] ?? ACP_AGENTS.default;
}
