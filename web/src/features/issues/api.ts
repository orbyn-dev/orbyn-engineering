import { useEffect, useState } from "react";
import type { IssuesApi, TrackerKind, TrackerRow } from "./contract";
import { useAsync } from "../../lib/useAsync";

/**
 * The real client for the issues routes. Those routes are not merged, so this one makes no request at all: the
 * service "reports nothing", the Issues entry stays out of the sidebar, no chip draws and no row has a marker.
 * Swap it for `request(...)` calls the day the generated types and routes land; the screens do not change.
 */
const unreported: IssuesApi = {
  trackers: async () => null,
  list: async () => ({ issues: [], counts: null, failed: [] }),
  get: async () => { throw new Error("Issues are not reported by this service."); },
  links: async () => ({}),
  find: async () => [],
  link: async () => { throw new Error("Issues are not reported by this service."); },
  unlink: async () => { throw new Error("Issues are not reported by this service."); },
  startThread: async () => { throw new Error("Issues are not reported by this service."); },
};

let current: IssuesApi = unreported;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/issues` installs the mock so the screens can be seen before the routes exist. In a
// production build the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/issues")) {
  void import("../../../mock/issues").then((module) => setIssuesApi(module.createIssuesMock({ delayMs: 400 })));
}

export const issuesApi = (): IssuesApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the unreported client back. */
export function setIssuesApi(next: IssuesApi | null): void {
  current = next ?? unreported;
  version += 1;
  for (const listener of listeners) listener();
}

/** Changes when the client is swapped, so a screen reads again from the new one. */
export function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** The trackers the service reports; `rows` is `null` while loading and when the service reports none at all. */
export function useTrackers(): { rows: TrackerRow[] | null; settled: boolean; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`issues:trackers:${seen}`, (signal) => issuesApi().trackers(signal));
  return { rows: read.status === "ready" ? read.data : null, settled: read.status !== "loading", reload: read.reload };
}

export type { TrackerKind };
