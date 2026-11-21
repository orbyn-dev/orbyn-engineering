/**
 * The repositories of a project and the writes the screens make, as the screens need them.
 *
 * GUESSED CONTRACT. No multi-repository route, Rust type or generated type is on main: Eng 4 is writing docs/multi-repo.md.
 * Nothing here is generated, and the only implementation of `ReposApi` besides the "reports nothing" client in api.ts is the
 * mock in web/mock/repos.ts. Field NAMES follow the T3 Code workspace folder schema (MIT, 9168ca73d, names only, no code
 * copied) in snake_case; every shape and every route a real client would call is a guess for Eng 4 to correct. When the
 * routes land, replace these hand-written types with the generated ones in the same commit that adds the real client.
 *
 * Nothing here can carry a secret. Names, paths, URLs and branch names are text from the disk or a remote: untrusted. Render
 * them as text only, never as a link, a command or markup.
 */

/** One folder of the project. */
export interface RepoRow {
  id: string;
  /** The folder's name, as the service shows it. */
  name: string;
  /** Where it is on disk. Shown as text. */
  path: string;
  is_git: boolean;
  /** Each repository is trusted on its own. */
  trusted: boolean;
  /** The checked-out branch; `null` for a folder that is not Git. */
  branch: string | null;
  /** The branches a thread may start from, current first. Empty when not Git. */
  branches: string[];
  /** A repository with no remote can be published. */
  has_remote: boolean;
  /** Present while the repository is being cloned, or when that clone failed. */
  clone: { state: "cloning" | "failed"; percent: number | null; message?: string | null } | null;
}

/** What the project's repository list holds, and the limits the service applies. */
export interface ReposView {
  repositories: RepoRow[];
  /** Where a clone is placed, shown in the clone dialog. `null` when the service has not chosen one. */
  clone_directory: string | null;
  /** The most repositories one thread may work in. */
  max_per_thread: number;
}

export interface PublishInput {
  /** `owner/name` on the host. */
  name: string;
  visibility: "private" | "public";
}

export interface StartRepoChoice {
  repo_id: string;
  /** The branch the thread's own branch starts from. */
  base_branch: string;
}

/** One repository a thread works in. */
export interface ThreadRepo {
  repo_id: string;
  name: string;
  /** The thread's own branch in this repository. */
  branch: string;
  added: number;
  deleted: number;
  trusted: boolean;
}

export type StartedThread = import("../../api/types").Thread;

export interface ReposApi {
  /** `null` is "this service has no multi-repository projects": the screens hide and no other call is made. */
  list(projectId: string, signal?: AbortSignal): Promise<ReposView | null>;
  /** One write per call: adds a folder in place. Nothing is copied. */
  addFolder(projectId: string, path: string): Promise<RepoRow>;
  /** One write per call: starts a clone that joins the project. It runs in the background. */
  clone(projectId: string, source: string): Promise<RepoRow>;
  /** One write per call: creates the repository on the host and pushes the current branch. */
  publish(projectId: string, repoId: string, input: PublishInput): Promise<RepoRow>;
  /** One write per call: trusts this folder only. */
  trust(projectId: string, repoId: string): Promise<RepoRow>;
  /** One write per call: runs `git init` in a folder that has none. */
  setupGit(projectId: string, repoId: string): Promise<RepoRow>;
  /** One write per call: stops tracking the repository in this project. Files and threads stay. */
  remove(projectId: string, repoId: string): Promise<void>;
  /** One write per call: creates the thread in these repositories, each on its own branch. */
  startThread(projectId: string, repos: StartRepoChoice[]): Promise<StartedThread>;
  /** The repositories a thread works in; `null` when the thread is not a multi-repository thread. */
  threadRepos(threadId: string, signal?: AbortSignal): Promise<ThreadRepo[] | null>;
  /** One write per call: adds one trusted repository of the project to the thread. */
  addThreadRepo(threadId: string, repoId: string): Promise<ThreadRepo>;
}
