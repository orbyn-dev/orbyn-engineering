import { describe, expect, it } from "vitest";
import type { EngineStatus, Project, Thread } from "../../api/types";
import { agentRow, listedAgents, noEngineReady, recentProjects } from "./welcomeModel";

const engine = (patch: Partial<EngineStatus>) =>
  ({ id: "claude", name: "Claude Code", tier: "managed", installed: true, auth: "logged_in", version: "2.1.318", ...patch }) as EngineStatus;

describe("agentRow", () => {
  it("is ready with the version and sign-in", () => {
    expect(agentRow(engine({}))).toMatchObject({ state: "ready", status: "Ready", detail: "2.1.318 · signed in" });
  });
  it("says an ACP agent runs over ACP", () => {
    expect(agentRow(engine({ tier: "acp", auth: "not_applicable", version: "1.4.0" }))).toMatchObject({ state: "ready", detail: "1.4.0 · runs over ACP" });
  });
  it("needs a sign-in, or is not found, with the one word Providers uses", () => {
    expect(agentRow(engine({ auth: "logged_out" }))).toMatchObject({ state: "signin", status: "Needs sign-in", detail: "Not signed in" });
    expect(agentRow(engine({ installed: false }))).toMatchObject({ state: "missing", status: "Not found", detail: "Not installed" });
  });
});

describe("listing", () => {
  it("leaves out an ACP agent that is not installed", () => {
    expect(listedAgents([engine({}), engine({ id: "x", tier: "acp", installed: false })]).map((entry) => entry.id)).toEqual(["claude"]);
  });
  it("says no engine is ready only when some are listed and none is", () => {
    expect(noEngineReady(null)).toBe(false);
    expect(noEngineReady([])).toBe(false);
    expect(noEngineReady([engine({ installed: false }), engine({ id: "codex", auth: "logged_out" })])).toBe(true);
    expect(noEngineReady([engine({ installed: false }), engine({ id: "codex" })])).toBe(false);
  });
});

describe("recentProjects", () => {
  const project = (id: string, created = "2026-01-01T00:00:00Z") => ({ id, name: id, path: `/${id}`, created_at: created }) as Project;
  const thread = (id: string, project_id: string, updated_at: string, archived = false) =>
    ({ id, project_id, updated_at, lifecycle: { archived_at: archived ? "x" : null } }) as unknown as Thread;
  it("puts the project that moved last first and counts its threads", () => {
    const rows = recentProjects(
      [project("a"), project("b"), project("c")],
      { 1: thread("1", "a", "2026-11-01T00:00:00Z"), 2: thread("2", "b", "2026-11-20T00:00:00Z"), 3: thread("3", "b", "2026-11-10T00:00:00Z"), 4: thread("4", "b", "2026-11-25T00:00:00Z", true) },
    );
    expect(rows.map((row) => [row.project.id, row.threads])).toEqual([["b", 2], ["a", 1], ["c", 0]]);
  });
  it("keeps only the first five", () => {
    expect(recentProjects(["1", "2", "3", "4", "5", "6"].map((id) => project(id)), {})).toHaveLength(5);
  });
});
