import { expect, it } from "vitest";
import { createMockApi } from "./api";
import type { ScopedPreferencesResult } from "../src/generated/ScopedPreferencesResult";
import type { PreferenceView } from "../src/api/types";

it("serves scoped reads for app fixtures and keeps stale selections unavailable", () => {
  const api = createMockApi();
  const initial = api.handle("POST", "/api/settings/preferences/query", { scope: {}, keys: ["branch_name_prefix"] });
  expect(initial.status).toBe(200);
  const result = initial.body as ScopedPreferencesResult;
  expect(result.catalog.selection).toEqual({ kind: "all_projects" });
  expect(result.rows[0]?.value).toBe("feat");
  const changed = api.handle("PUT", "/api/settings/preferences", {
    scope: {}, membership_version: result.catalog.version,
    if_match: { branch_name_prefix: result.rows[0]?.revision }, edits: { branch_name_prefix: { op: "set", value: "fix" } },
  });
  expect(changed.status).toBe(200);
  expect((changed.body as ScopedPreferencesResult).rows[0]?.value).toBe("fix");
  const unavailable = api.handle("POST", "/api/settings/project-scopes", { selection: { kind: "project_group", group_key: "removed" } });
  expect(unavailable.body).toMatchObject({ selection: { kind: "unavailable", reason: "project_missing" } });
  expect(api.handle("POST", "/api/settings/preferences/query", { scope: { selection: { kind: "project_group", group_key: "removed" } }, keys: ["branch_name_prefix"] }).status).toBe(409);
  api.reset();
});

it("returns typed model defaults and the same scoped override sources to the draft API", () => {
  const api = createMockApi();
  const read = (project?: string) => api.handle("GET", `/api/preferences${project ? `?project_id=${project}` : ""}`).body as PreferenceView[];
  expect(read()).toEqual(expect.arrayContaining([
    expect.objectContaining({ key: "default_engine", value: "", default: "", source: "default" }),
    expect.objectContaining({ key: "default_model", value: "", default: "", source: "default" }),
    expect.objectContaining({ key: "disabled_engines", value: ["cursor", "kimi"], default: ["cursor", "kimi"], source: "default" }),
  ]));
  const keys = ["default_engine", "default_model"];
  const query = api.handle("POST", "/api/settings/preferences/query", { scope: {}, keys }).body as ScopedPreferencesResult;
  api.handle("PUT", "/api/settings/preferences", { scope: {}, membership_version: query.catalog.version,
    if_match: Object.fromEntries(query.rows.map((row) => [row.key, row.revision])),
    edits: { default_engine: { op: "set", value: "codex" }, default_model: { op: "set", value: "environment" } },
  });
  const group = query.catalog.groups[0]!;
  const id = group.members[0]!.project_id;
  const scope = { selection: { kind: "checkout", group_key: group.key, project_id: id } };
  const selected = api.handle("POST", "/api/settings/preferences/query", { scope, keys }).body as ScopedPreferencesResult;
  api.handle("PUT", "/api/settings/preferences", { scope, membership_version: selected.catalog.version,
    if_match: Object.fromEntries(selected.rows.map((row) => [row.key, row.revision])),
    edits: { default_engine: { op: "set", value: "codex" }, default_model: { op: "set", value: "project" } },
  });
  expect(read(id).find((row) => row.key === "default_model")).toMatchObject({ value: "project", source: "project" });
  expect(read().find((row) => row.key === "default_model")).toMatchObject({ value: "environment", source: "global" });
  api.reset();
});

it("reads and saves a checkout's preferences through the legacy routes when checkouts of one repository are grouped", () => {
  const api = createMockApi();
  api.reset("settings-checkouts");
  for (const project of ["p-app", "p-app-fix"]) {
    const read = api.handle("GET", `/api/preferences?project_id=${project}`);
    expect(read.status, `GET for ${project}`).toBe(200);
    const saved = api.handle("PUT", `/api/projects/${project}/preferences`, { values: { branch_name_prefix: `hot-${project}` } });
    expect(saved.status, `PUT for ${project}`).toBe(200);
    expect((saved.body as PreferenceView[]).find((view) => view.key === "branch_name_prefix")).toMatchObject({ value: `hot-${project}`, source: "project" });
  }
  // A project that is its own group keeps working the same way, and an unknown one is still not found.
  expect(api.handle("GET", "/api/preferences?project_id=p-site").status).toBe(200);
  expect(api.handle("GET", "/api/preferences?project_id=nope").status).toBe(404);
  api.reset();
});
