import { useCallback, useEffect, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import type { ReposApi, ReposView, ThreadRepo } from "./contract";

const unavailable = () => { throw new Error("This service does not offer repositories in a project yet."); };

/**
 * The real client for the multi-repository routes. Those routes are not merged, so this one makes no request at all: the
 * service "reports nothing", the Repositories group stays out of project settings, no chip draws and a write says the service
 * does not offer it. Swap it for `request(...)` calls the day the generated types and routes land; the screens do not change.
 */
const unreported: ReposApi = {
  list: async () => null,
  addFolder: async () => unavailable(),
  clone: async () => unavailable(),
  publish: async () => unavailable(),
  trust: async () => unavailable(),
  setupGit: async () => unavailable(),
  remove: async () => unavailable(),
  startThread: async () => unavailable(),
  threadRepos: async () => null,
  addThreadRepo: async () => unavailable(),
  threadGit: async () => unavailable(),
  removeThreadRepo: async () => unavailable(),
};

let current: ReposApi = unreported;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/repos` installs the mock so the screens can be seen before the routes exist. In a production build the
// flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/repos")) {
  void import("../../../mock/repos").then((module) => setReposApi(module.createReposMock({ delayMs: 300 })));
}

export const reposApi = (): ReposApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the unreported client back. */
export function setReposApi(next: ReposApi | null): void {
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

/** The project's repositories; `view` is `null` while loading and when the service reports none at all. */
export function useRepos(projectId: string): { view: ReposView | null; settled: boolean; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`repos:${projectId}:${seen}`, (signal) => reposApi().list(projectId, signal));
  return { view: read.status === "ready" ? read.data : null, settled: read.status !== "loading", reload: read.reload };
}

/** The repositories one thread works in; `repos` is `null` while loading, for a thread in one repository or none, and when the service reports none. */
export function useThreadRepos(threadId: string, refreshKey = 0): { repos: ThreadRepo[] | null; reload(): void } {
  const version = useApiVersion();
  const [repos, setRepos] = useState<ThreadRepo[] | null>(null);
  const read = useCallback(() => {
    const abort = new AbortController();
    reposApi().threadRepos(threadId, abort.signal).then((next) => { if (!abort.signal.aborted) setRepos(next); }, () => { if (!abort.signal.aborted) setRepos(null); });
    return () => abort.abort();
  }, [threadId, version, refreshKey]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => read(), [read]);
  return { repos, reload: read };
}
