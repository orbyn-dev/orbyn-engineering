// The scoped preferences client: one read and one compare-and-set write, shared by every
// screen that saves a preference (Settings and the composer's quiet path switch).
import { request } from "./client";
import type { ScopedPreferencesResult } from "../generated/ScopedPreferencesResult";

export function queryPreferences(body: unknown, signal?: AbortSignal) {
  return request<ScopedPreferencesResult>("POST", "/api/settings/preferences/query", body, signal);
}

export function updatePreferences(body: unknown) {
  return request<ScopedPreferencesResult>("PUT", "/api/settings/preferences", body);
}
