import type { EngineStatus, Project, Thread } from "../../api/types";

export type AgentState = "ready" | "signin" | "missing";

export interface AgentRow {
  engine: EngineStatus;
  state: AgentState;
  /** The line under the name: the version and sign-in, or what is missing. */
  detail: string;
  /** The status word. The same words as Settings › Providers. */
  status: string;
}

/** The agents worth listing: every managed one, and an ACP one once it is installed. */
export const listedAgents = (engines: readonly EngineStatus[]): EngineStatus[] => engines.filter((engine) => engine.tier !== "acp" || engine.installed);

export function agentRow(engine: EngineStatus): AgentRow {
  if (!engine.installed) return { engine, state: "missing", detail: "Not installed", status: "Not found" };
  if (engine.auth === "logged_out") return { engine, state: "signin", detail: "Not signed in", status: "Needs sign-in" };
  const parts = [engine.version, engine.tier === "acp" ? "runs over ACP" : engine.auth === "logged_in" ? "signed in" : null].filter(Boolean);
  return { engine, state: "ready", detail: parts.length > 0 ? parts.join(" · ") : "Installed", status: "Ready" };
}

/** True once the engines are read, there are some to list, and none can start a thread. */
export const noEngineReady = (engines: readonly EngineStatus[] | null): boolean => {
  const rows = listedAgents(engines ?? []).map(agentRow);
  return rows.length > 0 && rows.every((row) => row.state !== "ready");
};

export interface RecentProject {
  project: Project;
  /** When the project's newest thread last moved; its creation when it has none. */
  lastAt: string;
  threads: number;
}

/** The projects with the one that moved last first, each with how many threads it holds. Archived threads are not counted. */
export function recentProjects(projects: readonly Project[], threads: Readonly<Record<string, Thread>>, limit = 5): RecentProject[] {
  const byProject = new Map<string, { lastAt: string; threads: number }>();
  for (const thread of Object.values(threads)) {
    if (thread.lifecycle?.archived_at) continue;
    const seen = byProject.get(thread.project_id) ?? { lastAt: "", threads: 0 };
    byProject.set(thread.project_id, { lastAt: thread.updated_at > seen.lastAt ? thread.updated_at : seen.lastAt, threads: seen.threads + 1 });
  }
  return projects
    .map((project) => ({ project, lastAt: byProject.get(project.id)?.lastAt || project.created_at, threads: byProject.get(project.id)?.threads ?? 0 }))
    .sort((a, b) => b.lastAt.localeCompare(a.lastAt))
    .slice(0, limit);
}
