/**
 * MOCK of the multi-repository routes, which are not merged on the service yet. It is never called against the real service:
 * the app's own client reports "nothing" until the routes exist, and only a test or the dev-only preview
 * (`#/preview/repos`) installs this through `setReposApi`. Fixtures follow the Engineer frames `t3gap-repos-*`; none holds a
 * credential, because the contract has no field for one. Every shape is a guess, see src/features/repos/contract.ts.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { Thread } from "../src/api/types.ts";
import type { PublishInput, PublishedRepo, ReposApi, RepoRow, StartThreadInput, ThreadRepo } from "../src/features/repos/contract.ts";

const row = (id: string, branch: string | null, extra: Partial<RepoRow> = {}): RepoRow => ({
  id, name: id, path: `~/Documents/Github/${id}`, is_git: branch !== null, trusted: true, branch,
  branches: branch === null ? [] : [branch, ...["main", "develop"].filter((name) => name !== branch)], has_remote: true, clone: null, ...extra,
});

export function repoFixtures(): RepoRow[] {
  return [
    row("orbyn-engineering", "main"),
    row("orbyn-planner", "feat/calendar", { has_remote: false }),
    row("orbyn-engine", "main", { trusted: false }),
    row("orbyn-notes", null, { trusted: false, has_remote: false }),
  ];
}

export interface ReposMock extends ReposApi {
  /** Every write the screens made, in order: `add:<path>`, `clone:<source>`, `publish:<repo>:<name>:<visibility>`, `trust:<repo>`,
   * `git:<repo>`, `remove:<repo>`, `start:<repo>@<base>,...`, `thread-add:<thread>:<repo>`,
   * `thread-git:<thread>:<repo>:<kind>`, `thread-remove:<thread>:<repo>`. */
  writes: string[];
  /** The engine, model and permission each `start` write carried, in order. */
  starts: Pick<StartThreadInput, "engine" | "model" | "permission_mode">[];
  /** Make the next write of that kind fail. */
  failNext(what: "add" | "clone" | "publish" | "trust" | "git" | "remove" | "start" | "thread-add" | "thread-git" | "thread-remove" | null): void;
}

export function createReposMock(init: {
  repositories?: RepoRow[] | null; threads?: Record<string, ThreadRepo[]>; delayMs?: number; maxPerThread?: number; cloneDirectory?: string | null;
} = {}): ReposMock {
  let repositories = init.repositories === undefined ? repoFixtures() : init.repositories;
  const threads: Record<string, ThreadRepo[]> = init.threads ?? {
    "thread-1": [
      { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "fix/login", added: 4, deleted: 3, trusted: true, files: ["auth/redirect.ts", "auth/redirect.test.ts"] },
      { repo_id: "orbyn-planner", name: "orbyn-planner", branch: "fix/login", added: 3, deleted: 0, trusted: true, files: ["calendar/feed.ts"] },
    ],
  };
  const writes: string[] = [];
  const starts: Pick<StartThreadInput, "engine" | "model" | "permission_mode">[] = [];
  let fail: string | null = null;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const take = (what: string) => {
    if (fail !== what) return;
    fail = null;
    throw new ApiFailure("repo_failed", "The service could not do that.", 502);
  };
  const held = () => repositories ?? [];
  const find = (id: string) => {
    const one = held().find((entry) => entry.id === id);
    if (!one) throw new ApiFailure("not_found", "No such repository in this project.", 404);
    return one;
  };
  const replace = (next: RepoRow) => { repositories = held().map((entry) => (entry.id === next.id ? next : entry)); return next; };
  return {
    writes,
    starts,
    failNext: (what) => { fail = what; },
    list: async () => {
      await wait();
      return repositories === null ? null : { repositories: held(), clone_directory: init.cloneDirectory === undefined ? "~/Documents/Github" : init.cloneDirectory, max_per_thread: init.maxPerThread ?? 8 };
    },
    addFolder: async (_project, path) => {
      writes.push(`add:${path}`);
      await wait();
      take("add");
      const name = path.split("/").filter(Boolean).pop() ?? path;
      const added = row(name, "main", { path, trusted: false });
      repositories = [...held(), added];
      return added;
    },
    clone: async (_project, source) => {
      writes.push(`clone:${source}`);
      await wait();
      take("clone");
      const name = source.replace(/\.git$/, "").split(/[/:]/).pop() ?? source;
      const added = row(name, "main", { trusted: false, clone: { state: "cloning", percent: 0 } });
      repositories = [...held(), added];
      return added;
    },
    publish: async (_project, repoId, input: PublishInput): Promise<PublishedRepo> => {
      writes.push(`publish:${repoId}:${input.name}:${input.visibility}${input.remote ? `:${input.remote}:${input.protocol ?? "ssh"}` : ""}`);
      await wait();
      take("publish");
      return { ...replace({ ...find(repoId), has_remote: true }), pushed: true };
    },
    trust: async (_project, repoId) => {
      writes.push(`trust:${repoId}`);
      await wait();
      take("trust");
      return replace({ ...find(repoId), trusted: true });
    },
    setupGit: async (_project, repoId) => {
      writes.push(`git:${repoId}`);
      await wait();
      take("git");
      return replace({ ...find(repoId), is_git: true, branch: "main", branches: ["main"] });
    },
    remove: async (_project, repoId) => {
      writes.push(`remove:${repoId}`);
      await wait();
      take("remove");
      find(repoId);
      repositories = held().filter((entry) => entry.id !== repoId);
    },
    startThread: async (projectId, { repos: choices, engine, model, permission_mode }) => {
      starts.push({ engine, model, permission_mode });
      writes.push(`start:${choices.map((choice) => `${choice.repo_id}@${choice.base_branch}`).join(",")}`);
      await wait();
      take("start");
      for (const choice of choices) {
        const one = find(choice.repo_id);
        if (!one.trusted || !one.is_git) throw new ApiFailure("untrusted_repository", `${one.name} is not trusted.`, 403);
      }
      const id = `thread-${writes.length + 100}`;
      threads[id] = choices.map((choice) => ({ repo_id: choice.repo_id, name: find(choice.repo_id).name, branch: "feat/new", added: 0, deleted: 0, trusted: true }));
      return { id, project_id: projectId, title: "New thread", engine, model, cwd: "/", worktree: null, skill_profile: null, active_run: null, last_run_state: null, created_at: new Date().toISOString(), updated_at: new Date().toISOString() } as Thread;
    },
    threadRepos: async (threadId) => { await wait(); return threads[threadId] ?? null; },
    addThreadRepo: async (threadId, repoId) => {
      writes.push(`thread-add:${threadId}:${repoId}`);
      await wait();
      take("thread-add");
      const one = find(repoId);
      if (!one.trusted) throw new ApiFailure("untrusted_repository", `${one.name} is not trusted.`, 403);
      const added: ThreadRepo = { repo_id: one.id, name: one.name, branch: "fix/login", added: 0, deleted: 0, trusted: true };
      threads[threadId] = [...(threads[threadId] ?? []), added];
      return added;
    },
    threadGit: async (threadId, repoId, action) => {
      writes.push(`thread-git:${threadId}:${repoId}:${action.kind}`);
      await wait();
      take("thread-git");
      const entry = (threads[threadId] ?? []).find((one) => one.repo_id === repoId);
      if (!entry) throw new ApiFailure("not_found", "This thread does not work in that repository.", 404);
      if (!entry.trusted) throw new ApiFailure("untrusted_repository", `${entry.name} is not trusted.`, 403);
      const next = action.kind === "pull_request" ? { ...entry, pull_request: { number: 143, url: `https://github.com/orbyn-dev/${entry.name}/pull/143` } } : entry;
      threads[threadId] = (threads[threadId] ?? []).map((one) => (one.repo_id === repoId ? next : one));
      return next;
    },
    removeThreadRepo: async (threadId, repoId) => {
      writes.push(`thread-remove:${threadId}:${repoId}`);
      await wait();
      take("thread-remove");
      threads[threadId] = (threads[threadId] ?? []).filter((one) => one.repo_id !== repoId);
    },
  };
}
