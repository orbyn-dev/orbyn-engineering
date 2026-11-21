/**
 * The repositories of a project and the writes the screens make: the shapes of docs/multi-repo.md (Eng 4, PR #923), written by hand
 * until `orbyn-contracts` generates them. When the generated types land, replace these in the same commit. Field names follow the
 * T3 Code workspace folder schema (MIT, 9168ca73d, names only) in snake_case.
 *
 * One thing the screens use is NOT in that document and is marked below: `ThreadRepo.pull_request`. The thread Git actions send
 * the existing thread routes with an optional `repo_id` ("Per-repository Git routes"). Removing a repository from a thread is not in
 * the beta, so there is no call for it.
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
  /** The project's own folder, its first repository (`id` equal to the project id). */
  primary: boolean;
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

/** What a publish sends. The service answers the updated `RepoRow`. The host (GitHub) is not a field: the page's provider step only chooses where to publish. */
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

/**
 * What the new-thread dialog sends. The permission mode is checked against the engine's `permission_modes`: a mode the engine does
 * not offer is refused with `permission_mode_unsupported` (400) before anything is created. A `model` of `null` is the engine's default.
 */
export interface StartThreadInput {
  repos: StartRepoChoice[];
  engine: string;
  model: string | null;
  permission_mode: import("../../api/types").PermissionMode;
  /** The optional first message. */
  message?: string;
  /** docs/multi-repo.md "Task folder": one new branch name cut in every chosen repository. Sent only for two or more repositories; absent, the service makes one name. */
  branch_name?: string;
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
  /** The paths of this repository's changed files, relative to the repository root, so two repositories can each have a `README.md`. Absent when the service has not read the changes yet. */
  files?: string[];
  /** NOT IN docs/multi-repo.md: the pull request of this repository's branch. The service sends none today, so the menu says "Create pull request". */
  pull_request?: { number: number; url: string } | null;
}

/**
 * One Git action on ONE repository of a thread; no action spans repositories. Each is an existing thread route that takes an
 * optional `repo_id` (docs/multi-repo.md, "Per-repository Git routes"): commit, push and pull request are `POST .../git/action`,
 * merge and review are `POST .../pulls/{number}/merge` and `.../review`. There is no pull (fetch) action: the document defines none.
 */
export type ThreadGitAction =
  | { kind: "commit"; message: string }
  | { kind: "push" }
  | { kind: "pull_request"; title: string }
  | { kind: "merge"; number: number; method: "merge" | "squash" | "rebase" }
  | { kind: "review"; number: number; event: "comment" | "approve" | "request_changes"; body: string };

/** The thread folder of a thread in 2 to `max_per_thread` repositories. `root` is shown as text only; a repository's worktree is `<root>/<name>`. */
export interface TaskFolder {
  root: string;
  /** The one branch name the thread uses in every repository. */
  branch: string;
  repos: ThreadRepo[];
  max_per_thread: number;
}

/** One entry of a folder scan or workspace import. `name` and `path` are untrusted text. */
export interface ScannedFolder {
  name: string;
  path: string;
  is_git: boolean;
  source: "child" | "code-workspace";
  /** Already a repository of a project. */
  in_project: boolean;
  /** Why it cannot be added by import; `null` when it can. */
  refused: "outside" | "missing" | "not_directory" | null;
}

/** The answer to a scan. Nothing is added or trusted by it. */
export interface RepoScan {
  path: string;
  is_git: boolean;
  /** The `.code-workspace` file the entries came from, or `null` for a folder scan. */
  code_workspace_file: string | null;
  folders: ScannedFolder[];
  /** More folders were there than the service returned. */
  truncated: boolean;
}

export type StartedThread = import("../../api/types").Thread;

/** `null` from a read is "this service has no multi-repository projects" (`404`): the screens hide and no write is made. Every write is Person-class and one write per call. */
export interface ReposApi {
  list(projectId: string, signal?: AbortSignal): Promise<ReposView | null>;
  /** Adds a folder in place. Nothing is copied. It is not trusted. */
  addFolder(projectId: string, path: string): Promise<RepoRow>;
  /** Makes a new folder named `name` under `clone_directory` and runs `git init` in it. */
  create(projectId: string, name: string): Promise<RepoRow>;
  /** Starts a clone that joins the project; it runs in the background (`clone.state: "cloning"`). The service chooses the destination. */
  clone(projectId: string, source: string): Promise<RepoRow>;
  /** Stops a running clone and removes its partial folder. */
  cancelClone(projectId: string, repoId: string): Promise<RepoRow>;
  /** Creates the repository on the host and pushes the current branch. */
  publish(projectId: string, repoId: string, input: PublishInput): Promise<RepoRow>;
  /** Trusts this folder only. The only way an untrusted repository becomes usable. */
  trust(projectId: string, repoId: string): Promise<RepoRow>;
  /** Runs `git init` in a folder that has none. */
  setupGit(projectId: string, repoId: string): Promise<RepoRow>;
  /** Stops tracking the repository in this project. Files, branches and threads stay. */
  remove(projectId: string, repoId: string): Promise<void>;
  /** Creates the thread in these repositories, each on its own branch (`POST /api/projects/{id}/repo-threads`). */
  startThread(projectId: string, input: StartThreadInput): Promise<StartedThread>;
  /** The repositories a thread works in; `null` (`404`) when the thread is not a multi-repository thread. */
  threadRepos(threadId: string, signal?: AbortSignal): Promise<ThreadRepo[] | null>;
  /**
   * Reads one folder (or a `.code-workspace` file) and lists what could be added: `POST /api/projects/{id}/repos/scan`, Person-class,
   * writes nothing. `null` (`404`) means the service has no scan. A newer call supersedes an older one through `signal`.
   */
  scan(projectId: string, path: string, signal?: AbortSignal): Promise<RepoScan | null>;
  /** The thread's task folder; `null` (`404`) when the thread has none or the service does not know the route. */
  taskFolder(threadId: string, signal?: AbortSignal): Promise<TaskFolder | null>;
  /** Adds one trusted repository of the project to the thread. */
  addThreadRepo(threadId: string, repoId: string): Promise<ThreadRepo>;
  /**
   * One Git action in one repository of the thread. `repoId` is `null` for the project's primary repository: `repo_id` is then left
   * out and the service uses the primary. A repository that is not the thread's is `404 not_found`; an untrusted one is `409 repo_untrusted`.
   */
  threadGit(threadId: string, repoId: string | null, action: ThreadGitAction): Promise<void>;
}
