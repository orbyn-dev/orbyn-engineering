import type { MockApi } from "../../../../mock/api";
import type { JsonValue } from "../../../api/types";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";

/** Sets one preference for every project, as the person saved it earlier: through the service's own save, not into the page. */
export function seedPreference(mock: MockApi, key: string, value: JsonValue) {
  const scope = {};
  const before = mock.handle("POST", "/api/settings/preferences/query", { scope, keys: [key] }).body as ScopedPreferencesResult;
  const saved = mock.handle("PUT", "/api/settings/preferences", {
    scope, membership_version: before.catalog.version,
    if_match: { [key]: before.rows[0]?.revision ?? "" }, edits: { [key]: { op: "set", value } },
  });
  if (saved.status !== 200) throw new Error(`The mock refused to seed ${key}: ${JSON.stringify(saved.body)}`);
}
