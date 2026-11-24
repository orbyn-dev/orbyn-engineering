/**
 * MOCK of the multi-repository routes of docs/multi-repo.md, for tests and the dev-only preview (`#/preview/repos`), which install
 * it through `setReposApi`. The app's own client is the real HTTP one in src/features/repos/api.ts. Fixtures follow the Engineer
 * frames `t3gap-repos-*`; none holds a credential, because the contract has no field for one. It refuses what the service refuses:
 * an untrusted repository (`repo_untrusted`), a thread over the limit (`repo_limit`), a clone source the doc forbids
 * (`clone_refused`) and a permission mode the engine does not offer (`permission_mode_unsupported`). A thread start follows
 * docs/multi-repo.md "As built: repository threads and the task folder": more than 8 repositories is `409 repo_limit`, fewer
 * than 2 or a repository chosen twice or a base branch that is not a branch of it or a branch name already taken is `400`,
 * an untrusted repository is `409 repo_untrusted` and a folder with no Git is `409 not_git`; nothing is created on a refusal.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { PermissionMode, Thread } from "../src/api/types.ts";
import type { PublishInput, RepoScan, ReposApi, RepoRow, StartThreadInput, TaskFolder, ThreadRepo } from "../src/features/repos/contract.ts";

const row = (id: string, branch: string | null, extra: Partial<RepoRow> = {}): RepoRow => ({
  id, name: id, path: `~/Documents/Github/${id}`, is_git: branch !== null, trusted: true, branch,
  branches: branch === null ? [] : [branch, ...["main", "develop"].filter((name) => name !== branch)], has_remote: true, primary: false, clone: null, ...extra,
});

export function repoFixtures(): RepoRow[] {
  return [
    row("orbyn-engineering", "main", { primary: true }),
    row("orbyn-planner", "feat/calendar", { has_remote: false }),
    row("orbyn-engine", "main", { trusted: false }),
    row("orbyn-notes", null, { trusted: true, has_remote: false }),
  ];
}

export interface ReposMock extends ReposApi {
  /** Every write the screens made, in order: `add:<path>`, `clone:<source>`, `publish:<repo>:<name>:<visibility>`, `trust:<repo>`,
   * `git:<repo>`, `create:<name>`, `cancel-clone:<repo>`, `remove:<repo>`, `start:<repo>@<base>,...`, `thread-add:<thread>:<repo>`, `thread-remove:<thread>:<repo>`,
   * `thread-git:<thread>:<repo or primary>:<kind>`. */
  writes: string[];
  /** The engine, model, permission and first message each `start` write carried, in order. */
  starts: Pick<StartThreadInput, "engine" | "model" | "permission_mode" | "message">[];
  /** Every scan the screens asked for, in order: `scan:<path>`. A scan writes nothing. */
  scans: string[];
  /** The `branch_name` each `start` write carried (`undefined` when none), in order. */
  branchNames: (string | undefined)[];
  /** Flips a repository to untrusted behind the screens' back, as if it were revoked elsewhere: their list is stale until they read again. */
  distrust(repoId: string): void;
  /** The next write of that kind fails with exactly this refusal (the service's own code, status and message). */
  rejectNext(what: "start" | "thread-add" | "thread-remove", failure: ApiFailure): void;
  /** Make the next write of that kind fail. */
  failNext(what: "add" | "create" | "cancel-clone" | "clone" | "publish" | "trust" | "git" | "remove" | "start" | "thread-add" | "thread-remove" | "thread-git" | null): void;
}

export function createReposMock(init: {
  repositories?: RepoRow[] | null; threads?: Record<string, ThreadRepo[]>; permissionModes?: Record<string, PermissionMode[]>; delayMs?: number; publish?: "unsupported" | "works"; maxPerThread?: number; takenBranches?: Record<string, string[]>; cloneDirectory?: string | null; scan?: (path: string) => RepoScan | null;
} = {}): ReposMock {
  let repositories = init.repositories === undefined ? repoFixtures() : init.repositories;
  const threads: Record<string, ThreadRepo[]> = init.threads ?? {
    "thread-1": [
      { repo_id: "orbyn-engineering", name: "orbyn-engineering", branch: "fix/login", added: 4, deleted: 3, trusted: true, files: ["auth/redirect.ts", "auth/redirect.test.ts"] },
      { repo_id: "orbyn-planner", name: "orbyn-planner", branch: "fix/login", added: 3, deleted: 0, trusted: true, files: ["calendar/feed.ts"] },
    ],
  };
  const writes: string[] = [];
  const scans: string[] = [];
  const branchNames: (string | undefined)[] = [];
  const starts: Pick<StartThreadInput, "engine" | "model" | "permission_mode" | "message">[] = [];
  let fail: string | null = null;
  const refusals = new Map<string, ApiFailure>();
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const take = (what: string) => {
    const refusal = refusals.get(what);
    if (refusal) { refusals.delete(what); throw refusal; }
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
    scans,
    branchNames,
    starts,
    failNext: (what) => { fail = what; },
    distrust: (repoId) => { replace({ ...find(repoId), trusted: false }); },
    rejectNext: (what, failure) => { refusals.set(what, failure); },
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
    create: async (_project, name) => {
      writes.push(`create:${name}`);
      await wait();
      take("create");
      if (!/^[A-Za-z0-9_][\w.-]*$/.test(name) || name.includes("..")) throw new ApiFailure("invalid", "That is not a folder name.", 400);
      // The service: `create` starts trusted (Orbyn made an empty folder); `add-folder` and `clone` start untrusted.
      const added = row(name, "main", { path: `~/Documents/Github/${name}`, trusted: true, has_remote: false });
      repositories = [...held(), added];
      return added;
    },
    cancelClone: async (_project, repoId) => {
      writes.push(`cancel-clone:${repoId}`);
      await wait();
      take("cancel-clone");
      const one = find(repoId);
      if (one.clone?.state !== "cloning") throw new ApiFailure("invalid", "That repository is not being cloned.", 400);
      // The service answers the row as failed, "Cancelled."; it stays listed until it is removed.
      const cancelled: RepoRow = { ...one, is_git: false, trusted: false, branch: null, branches: [], has_remote: false, clone: { state: "failed", percent: null, message: "Cancelled." } };
      return replace(cancelled);
    },
    clone: async (_project, source) => {
      writes.push(`clone:${source}`);
      await wait();
      take("clone");
      if (!/^(https:\/\/|ssh:\/\/|[\w.-]+@[\w.-]+:)[^\s]+$/.test(source)) throw new ApiFailure("clone_refused", "That source cannot be cloned.", 400);
      const name = source.replace(/\.git$/, "").split(/[/:]/).pop() ?? source;
      const added = row(name, "main", { trusted: false, clone: { state: "cloning", percent: null } });
      repositories = [...held(), added];
      return added;
    },
    publish: async (_project, repoId, input: PublishInput) => {
      writes.push(`publish:${repoId}:${input.name}:${input.visibility}`);
      await wait();
      take("publish");
      if (!find(repoId).trusted) throw new ApiFailure("repo_untrusted", "Trust this repository before publishing it.", 409);
      // The service today: 422 unsupported after the trust check.
      if ((init.publish ?? "unsupported") === "unsupported") throw new ApiFailure("unsupported", "Publishing a repository arrives in a later update.", 422);
      return replace({ ...find(repoId), has_remote: true });
    },
    trust: async (_project, repoId) => {
      writes.push(`trust:${repoId}`);
      await wait();
      take("trust");
      if (find(repoId).clone !== null) throw new ApiFailure("invalid", "Wait for the clone to finish.", 400);
      return replace({ ...find(repoId), trusted: true });
    },
    setupGit: async (_project, repoId) => {
      writes.push(`git:${repoId}`);
      await wait();
      take("git");
      if (!find(repoId).trusted) throw new ApiFailure("repo_untrusted", "Trust this folder before setting up Git.", 409);
      return replace({ ...find(repoId), is_git: true, branch: "main", branches: ["main"] });
    },
    remove: async (_project, repoId) => {
      writes.push(`remove:${repoId}`);
      await wait();
      take("remove");
      find(repoId);
      repositories = held().filter((entry) => entry.id !== repoId);
    },
    startThread: async (projectId, { repos: choices, engine, model, permission_mode, message, branch_name }) => {
      branchNames.push(branch_name ?? undefined);
      starts.push({ engine, model, permission_mode, ...(message === undefined ? {} : { message }) });
      writes.push(`start:${choices.map((choice) => `${choice.repo_id}@${choice.base_branch}`).join(",")}`);
      await wait();
      take("start");
      const limit = init.maxPerThread ?? 8;
      if (choices.length > limit) throw new ApiFailure("repo_limit", `A thread works in at most ${limit} repositories.`, 409);
      if (choices.length < 2) throw new ApiFailure("invalid", "Choose two or more repositories; one repository uses the normal thread start.", 400);
      const seen = new Set<string>();
      for (const choice of choices) {
        if (seen.has(choice.repo_id)) throw new ApiFailure("invalid", "A repository is chosen twice.", 400);
        seen.add(choice.repo_id);
        const one = find(choice.repo_id);
        if (one.clone !== null) throw new ApiFailure("invalid", `${one.name} is still being cloned.`, 400);
        if (!one.trusted) throw new ApiFailure("repo_untrusted", `Trust ${one.name} before starting a thread in it.`, 409);
        if (!one.is_git) throw new ApiFailure("not_git", `${one.name} is not a Git repository.`, 409);
        if (!one.branches.includes(choice.base_branch)) throw new ApiFailure("invalid", `${choice.base_branch.slice(0, 80)} is not a branch of ${one.name}.`, 400);
      }
      const offeredModes = init.permissionModes?.[engine];
      if (offeredModes && !offeredModes.includes(permission_mode)) throw new ApiFailure("permission_mode_unsupported", `${engine} does not offer the ${permission_mode} permission mode.`, 400);
      const id = `thread-${writes.length + 100}`;
      const branch = branch_name?.trim() || `orbyn/${id.slice(0, 8)}`;
      const taken = choices.find((choice) => (init.takenBranches?.[choice.repo_id] ?? []).includes(branch));
      if (taken) throw new ApiFailure("invalid", `${find(taken.repo_id).name} already has a branch named ${branch}.`, 400);
      threads[id] = choices.map((choice) => ({ repo_id: choice.repo_id, name: find(choice.repo_id).name, branch, added: 0, deleted: 0, trusted: true }));
      return { id, project_id: projectId, title: "New thread", engine, model, cwd: `~/Library/Application Support/Orbyn/threads/${id}`, worktree: null, skill_profile: null, active_run: null, last_run_state: null, created_at: new Date().toISOString(), updated_at: new Date().toISOString() } as Thread;
    },
    scan: async (_project, path, signal) => {
      scans.push(`scan:${path}`);
      await wait();
      if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
      if (init.scan !== undefined) return init.scan(path);
      const workspace = path.endsWith(".code-workspace");
      const folders: RepoScan["folders"] = [
        { name: "orbyn-docs", path: `${path}/orbyn-docs`, is_git: true, source: workspace ? "code_workspace" : "child", in_project: false, refused: null },
        { name: "orbyn-planner", path: `${path}/orbyn-planner`, is_git: true, source: workspace ? "code_workspace" : "child", in_project: true, refused: null },
        { name: "notes", path: `${path}/notes`, is_git: false, source: workspace ? "code_workspace" : "child", in_project: false, refused: null },
        ...(workspace ? [{ name: "shared", path: "/elsewhere/shared", is_git: true, source: "code_workspace" as const, in_project: false, refused: "outside" as const }] : []),
      ];
      return { path, is_git: false, code_workspace_file: workspace ? path : null, folders, truncated: false };
    },
    taskFolder: async (threadId) => {
      await wait();
      const held = threads[threadId] ?? [];
      return held.length >= 2 ? ({ root: `~/Library/Application Support/Orbyn/threads/${threadId}`, branch: held[0]!.branch, repos: held, max_per_thread: init.maxPerThread ?? 8 } satisfies TaskFolder) : null;
    },
    threadRepos: async (threadId) => { await wait(); return threads[threadId] ?? null; },
    addThreadRepo: async (threadId, repoId) => {
      writes.push(`thread-add:${threadId}:${repoId}`);
      await wait();
      take("thread-add");
      const held = threads[threadId];
      if (!held || held.length === 0) throw new ApiFailure("not_found", "No such repositories.", 404);
      const limit = init.maxPerThread ?? 8;
      if (held.length >= limit) throw new ApiFailure("repo_limit", `A thread works in at most ${limit} repositories.`, 409);
      if (held.some((entry) => entry.repo_id === repoId)) throw new ApiFailure("invalid", "That repository is already in this thread.", 400);
      const one = find(repoId);
      if (one.clone !== null) throw new ApiFailure("invalid", `${one.name} is still being cloned.`, 400);
      if (!one.trusted) throw new ApiFailure("repo_untrusted", `Trust ${one.name} before starting a thread in it.`, 409);
      if (!one.is_git) throw new ApiFailure("not_git", `${one.name} is not a Git repository.`, 409);
      const branch = held[0]!.branch;
      if ((init.takenBranches?.[repoId] ?? []).includes(branch)) throw new ApiFailure("invalid", `${one.name} already has a branch named ${branch}.`, 400);
      const added: ThreadRepo = { repo_id: one.id, name: one.name, branch, added: 0, deleted: 0, trusted: true };
      threads[threadId] = [...held, added];
      return added;
    },
    removeThreadRepo: async (threadId, repoId) => {
      writes.push(`thread-remove:${threadId}:${repoId}`);
      await wait();
      take("thread-remove");
      const held = threads[threadId] ?? [];
      const entry = held.find((one) => one.repo_id === repoId);
      if (!entry) throw new ApiFailure("not_found", "This thread does not work in that repository.", 404);
      // The primary is the project's own repository, the thread's first here; it never leaves.
      if (entry === held[0]) throw new ApiFailure("invalid", "The primary repository cannot be removed from a thread.", 400);
      if (entry.added + entry.deleted > 0) throw new ApiFailure("invalid", "Commit or discard its changes first", 400);
      threads[threadId] = held.filter((one) => one !== entry);
    },
    threadGit: async (threadId, repoId, action) => {
      writes.push(`thread-git:${threadId}:${repoId ?? "primary"}:${action.kind}`);
      await wait();
      take("thread-git");
      const held = threads[threadId] ?? [];
      // No repo_id is the primary repository: the thread's first.
      const entry = repoId === null ? held[0] : held.find((one) => one.repo_id === repoId);
      if (!entry) throw new ApiFailure("not_found", "This thread does not work in that repository.", 404);
      if (!entry.trusted) throw new ApiFailure("repo_untrusted", `${entry.name} is not trusted.`, 409);
    },
  };
}
