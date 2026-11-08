import type { EngineTools, ToolGroup } from "../../api/types";

/** Groups whose entries count toward "ready": Built-in tools and Skills are listed, not counted. */
const COUNTED = new Set(["orbyn", "mcp", "connectors", "plugins"]);

export const GROUP_LABEL: Record<string, string> = {
  builtin: "Built-in",
  orbyn: "Orbyn",
  mcp: "Your MCP servers",
  connectors: "Connectors",
  plugins: "Plugins",
  skills: "Skills",
};

/** What a missing group is called in "<Engine> doesn’t report …". */
const WHAT_IT_REPORTS: Record<string, string> = {
  builtin: "its built-in tools",
  orbyn: "Orbyn’s tools",
  mcp: "its MCP servers",
  connectors: "its connectors",
  plugins: "its plugins",
  skills: "its skills",
};

export interface ToolsSummary {
  ready: number;
  starting: number;
  needsSignIn: number;
  failed: number;
}

/** Counts across the groups that count. Orbyn's own tools are listed by number alone and are ready. */
export function summarize(tools: EngineTools): ToolsSummary {
  const total: ToolsSummary = { ready: 0, starting: 0, needsSignIn: 0, failed: 0 };
  for (const group of tools.groups) {
    if (!COUNTED.has(group.id)) continue;
    if (group.items.length === 0) {
      total.ready += group.count;
      continue;
    }
    for (const item of group.items) {
      if (item.state === "ready") total.ready += 1;
      else if (item.state === "starting") total.starting += 1;
      else if (item.state === "needs_sign_in") total.needsSignIn += 1;
      else total.failed += 1;
    }
  }
  return total;
}

/** The details card's second line: only what is not ready, in words. */
export function problemLine(summary: ToolsSummary): string | null {
  const parts = [
    summary.needsSignIn > 0 ? `${summary.needsSignIn} needs sign-in` : null,
    summary.failed > 0 ? `${summary.failed} failed` : null,
    summary.starting > 0 ? `${summary.starting} starting` : null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** Quiet lines for groups the engine does not report; a group nothing can report is not named. */
export function notReportedNotes(engineName: string, tools: EngineTools): string[] {
  return tools.not_reported
    .filter((id) => id in WHAT_IT_REPORTS)
    .map((id) => `${engineName} doesn’t report ${WHAT_IT_REPORTS[id]}.`);
}

export const groupOf = (tools: EngineTools, id: string): ToolGroup | undefined => tools.groups.find((group) => group.id === id);
