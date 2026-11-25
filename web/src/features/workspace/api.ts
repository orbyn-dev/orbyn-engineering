// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
import { request } from "../../api/client";
import type { ProjectScopeCatalog } from "../../generated/ProjectScopeCatalog";

/** The device's plain preference rows. Reads made together share one request. */
export { readPreferences } from "../../api/preferences";

export function queryProjectScopes(body: unknown, signal?: AbortSignal) {
  return request<ProjectScopeCatalog>("POST", "/api/settings/project-scopes", body, signal);
}

export { api } from "../../api/client";
export { pullsApi } from "../../api/pulls";
export { inDesktopApp, readShellStatus } from "../../api/shell";
