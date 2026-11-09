// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
import { request } from "../../api/client";
import type { PreferenceView } from "../../api/types";
import type { ScopedPreferencesResult } from "../../generated/ScopedPreferencesResult";

const q = encodeURIComponent;

export function readPreferences(projectId: string | null = null, signal?: AbortSignal) {
  const path = projectId === null ? "/api/preferences" : `/api/preferences?project_id=${q(projectId)}`;
  return request<PreferenceView[]>("GET", path, undefined, signal);
}

export function queryPreferences(body: unknown, signal?: AbortSignal) {
  return request<ScopedPreferencesResult>("POST", "/api/settings/preferences/query", body, signal);
}

export function updatePreferences(body: unknown) {
  return request<ScopedPreferencesResult>("PUT", "/api/settings/preferences", body);
}

export { api } from "../../api/client";
