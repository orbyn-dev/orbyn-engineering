// The scoped preferences client: one read and one compare-and-set write, shared by every
// screen that saves a preference (Settings and the composer's quiet path switch).
import { request } from "./client";
import type { PreferenceView } from "./types";
import type { ScopedPreferencesResult } from "../generated/ScopedPreferencesResult";

/** The plain preference rows, for the device or for one project. Reads made together share one request. */
export function readPreferences(projectId: string | null = null, signal?: AbortSignal) {
  const path = projectId === null ? "/api/preferences" : `/api/preferences?project_id=${encodeURIComponent(projectId)}`;
  return request<PreferenceView[]>("GET", path, undefined, signal, { share: true });
}

/** A read-shaped POST: the same scope and keys asked for together share one slow handler call. */
export function queryPreferences(body: unknown, signal?: AbortSignal) {
  return request<ScopedPreferencesResult>("POST", "/api/settings/preferences/query", body, signal, { share: true });
}

export function updatePreferences(body: unknown) {
  return request<ScopedPreferencesResult>("PUT", "/api/settings/preferences", body);
}
