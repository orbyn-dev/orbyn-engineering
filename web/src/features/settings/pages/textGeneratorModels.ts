import type { ProjectScopesQuery } from "../../../generated/ProjectScopesQuery";
import type { ScopedPreferenceRow } from "../../../generated/ScopedPreferenceRow";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";

export const TEXT_GENERATOR_MODELS = "text_generation_models";

/**
 * The map this scope itself names, which is what a write replaces. At project scope the
 * scoped row's `value`/members are the project's effective map (its own entries layered
 * over the global ones), so read the project's own entry from `overrides` instead; no
 * override means the project names no engine. Without that entry the fallback would be
 * the engines with `project.own === true` and their `project.chosen`, but that list only
 * holds enabled engines, so building from it would drop a turned-off engine's choice.
 */
export function storedTextGeneratorModels(row: ScopedPreferenceRow, project: string | null): Record<string, string> {
  const value = project === null
    ? row.value
    : row.overrides.find((entry) => entry.project_id === project)?.value;
  if (value === null || typeof value !== "object" || Array.isArray(value)) return {};
  return Object.fromEntries(Object.entries(value)
    .filter((entry): entry is [string, string] => typeof entry[1] === "string"));
}

/**
 * The complete map a write sends: the service replaces the object rather than merging
 * engine entries, so every other choice (including one for an engine the list leaves
 * out) is carried along. Null is Automatic: remove just this engine, and say so with
 * `null` when the scope's own map does not name it, so no empty write is made.
 */
export function textGeneratorChange(result: ScopedPreferencesResult, scope: ProjectScopesQuery,
  project: string | null, engine: string, model: string | null): ScopedPreferencesChange | null {
  const row = result.rows.find((entry) => entry.key === TEXT_GENERATOR_MODELS);
  if (!row) throw new Error("Text generator choices are unavailable.");
  const models = storedTextGeneratorModels(row, project);
  if (model === null && !(engine in models)) return null;
  if (model === null) delete models[engine];
  else models[engine] = model;
  return {
    scope,
    membership_version: result.catalog.version,
    if_match: { [TEXT_GENERATOR_MODELS]: row.revision },
    edits: { [TEXT_GENERATOR_MODELS]: { op: "set", value: models } },
  };
}
