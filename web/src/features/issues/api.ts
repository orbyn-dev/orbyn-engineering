import { useEffect, useRef, useState } from "react";
import type { IssuesApi, TrackerKind, TrackerRow } from "./contract";
import { createLiveIssuesApi } from "./http";
import { useAsync } from "../../lib/useAsync";

/**
 * The app's client is the live one (http.ts): it asks `GET /api/issues/trackers` and, on a 404 (the Issues flag is off, or the
 * routes do not exist), stays quiet: the service "reports nothing", the Issues entry stays out of the sidebar, no chip draws, no
 * row has a marker and no error shows. Any other answer turns the real routes on. Tests and the dev-only preview install the mock.
 */
let current: IssuesApi = createLiveIssuesApi();
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/issues` installs the mock so the screens can be seen before the routes exist. In a
// production build the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/issues")) {
  void import("../../../mock/issues").then((module) => setIssuesApi(module.createIssuesMock({ delayMs: 400 })));
}

export const issuesApi = (): IssuesApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts a fresh live client back, which asks the service again. */
export function setIssuesApi(next: IssuesApi | null): void {
  current = next ?? createLiveIssuesApi();
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
export function useTrackers(): { rows: TrackerRow[] | null; settled: boolean; reload(): void; refresh(): void; replace(next: TrackerRow): void } {
  const seen = useApiVersion();
  const read = useAsync(`issues:trackers:${seen}`, (signal) => issuesApi().trackers(signal));
  const rows = read.status === "ready" ? read.data : null;
  // A write answers with the tracker's own row: it is put in place where it stands, and the others are left as they are.
  // Every answer is applied to the rows as they are when it lands, not as they were at the click.
  const latest = useRef<TrackerRow[] | null>(rows);
  latest.current = rows;
  const { setData } = read;
  const replace = (next: TrackerRow) => {
    const updated = (latest.current ?? []).map((row) => (row.tracker === next.tracker ? next : row));
    latest.current = updated;
    setData(updated);
  };
  return { rows, settled: read.status !== "loading", reload: read.reload, refresh: () => void read.refreshQuietly(), replace };
}

export type { TrackerKind };
