import { useCallback, useEffect, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import { ApiFailure, request } from "../../api/client";
import type { Thread } from "../../api/types";
import type { PublishInput, ReposApi, ReposView, RepoRow, StartThreadInput, ThreadGitAction, ThreadRepo } from "./contract";

const q = encodeURIComponent;
const notOffered = (what: string) => new ApiFailure("not_offered", `This service does not offer ${what} yet.`, 0);
const isMissing = (error: unknown) => error instanceof ApiFailure && error.status === 404;

/** `false` once the service answered the repository list with `404`: the flag is off or the routes are missing. */
let offered: boolean | null = null;

/**
 * The real client for the routes of docs/multi-repo.md. The page asks for the project's list first; a `404` is "this service has
 * no multi-repository projects" (flag off or routes missing), answered as `null` with no error, so nothing draws and no write is
 * ever offered. Every write is Person-class (`personKey.ts`): `request` adds the key for each. Ids are encoded into the path.
 * The thread Git actions and removing a repository from a thread are not in that document, so they send nothing.
 */
const http: ReposApi = {
  async list(projectId, signal) {
    try {
      const view = await request<ReposView>("GET", `/api/projects/${q(projectId)}/repos`, undefined, signal);
      // An answer that is not a repository list (some other service behind the same route) is "nothing", not a crash.
      if (typeof view !== "object" || view === null || !Array.isArray(view.repositories)) return null;
      offered = true;
      return view;
    } catch (error) {
      if (!isMissing(error)) throw error;
      offered = false;
      return null;
    }
  },
  addFolder: (projectId, path) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/add-folder`, { path }),
  create: (projectId, name) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/create`, { name }),
  clone: (projectId, source) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/clone`, { source }),
  cancelClone: (projectId, repoId) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/${q(repoId)}/clone/cancel`),
  publish: (projectId, repoId, input: PublishInput) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/${q(repoId)}/publish`, { name: input.name, visibility: input.visibility }),
  trust: (projectId, repoId) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/${q(repoId)}/trust`),
  setupGit: (projectId, repoId) => request<RepoRow>("POST", `/api/projects/${q(projectId)}/repos/${q(repoId)}/setup-git`),
  async remove(projectId, repoId) { await request<void>("POST", `/api/projects/${q(projectId)}/repos/${q(repoId)}/remove`); },
  startThread: (projectId, input: StartThreadInput) => request<Thread>("POST", `/api/projects/${q(projectId)}/repo-threads`, input),
  async threadRepos(threadId, signal) {
    if (offered === false) return null;
    try {
      return await request<ThreadRepo[]>("GET", `/api/threads/${q(threadId)}/repos`, undefined, signal);
    } catch (error) {
      if (isMissing(error)) return null;
      throw error;
    }
  },
  addThreadRepo: (threadId, repoId) => request<ThreadRepo>("POST", `/api/threads/${q(threadId)}/repos`, { repo_id: repoId }),
  threadGit: async (_threadId: string, _repoId: string, _action: ThreadGitAction) => { throw notOffered("Git actions on one repository of a thread"); },
  removeThreadRepo: async () => { throw notOffered("removing a repository from a thread"); },
};

let current: ReposApi = http;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/repos` installs the mock so the screens can be seen before the routes exist. In a production build the
// flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/repos")) {
  void import("../../../mock/repos").then((module) => setReposApi(module.createReposMock({ delayMs: 300 })));
}

export const reposApi = (): ReposApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setReposApi(next: ReposApi | null): void {
  current = next ?? http;
  offered = null;
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
