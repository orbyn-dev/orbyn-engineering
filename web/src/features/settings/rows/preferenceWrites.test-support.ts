import { vi } from "vitest";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";

/** Every settings save sent through the stubbed `fetch` since `installMockFetch()`, as the service received it. */
export function preferenceWrites(): ScopedPreferencesChange[] {
  return vi.mocked(fetch).mock.calls
    .filter(([url, init]) => String(url) === "/api/settings/preferences" && init?.method === "PUT")
    .map(([, init]) => JSON.parse(String(init?.body)) as ScopedPreferencesChange);
}

/** The edits of each save, in the order they were sent. */
export const sentEdits = () => preferenceWrites().map((write) => write.edits);
