/**
 * The repositories of a project and the writes the screens make. The project repository routes (list, add-folder, create, clone,
 * cancel, scan, trust, setup-git, remove) and the thread side (`ThreadRepo`, `StartThreadInput`, `StartRepoChoice`, `AddThreadRepo`,
 * `TaskFolder`: `repo-threads`, `task-folder`, `GET`/`POST /api/threads/{id}/repos`) use the generated types below, read against
 * `crates/orbynd/src/routes/repos.rs` and `crates/orbyn-core/src/repos/threads.rs`. Field names follow the T3 Code workspace folder
 * schema (MIT, 9168ca73d, names only) in snake_case.
 *
 * The thread Git actions send the existing thread routes with an optional `repo_id` (`GitAction.repo_id`). Removing a repository from
 * a thread is `removeThreadRepo`. The thread pull request routes take an optional `repo_id` too (`pullsApi`), so each repository's pull request,
 * review and merge are offered in its own Git menu.
 *
 * Nothing here can carry a secret. Names, paths, URLs and branch names are text from the disk or a remote: untrusted. Render
 * them as text only, never as a link, a command or markup.
 */

/**
 * The list, the row, the clone state and the scan are the service's generated types (`orbyn-contracts`, `src/generated`): same names,
 * re-exported here so the screens keep one import. Differences from the guess this file once held, all taken from the generated types
 * and `crates/orbynd/src/routes/repos.rs`:
 * - `ScannedFolder.source` is `"child" | "code_workspace"` (underscore); the guess and docs/multi-repo.md said `code-workspace`.
 * - `RepoClone.percent` is optional and always `null` or absent: no screen reads it. A clone has no percentage.
 * - `RepoClone.state` is `"cloning" | "failed"`; a cancelled clone is `failed` with the message `Cancelled.`.
 * - `RepoRow.primary` exists (the project's own folder, `id` equal to the project id).
 */
export type { RepoClone } from "../../generated/RepoClone";
export type { RepoRow } from "../../generated/RepoRow";
export type { ReposView } from "../../generated/ReposView";
export type { RepoScan } from "../../generated/RepoScan";
export type { ScannedFolder } from "../../generated/ScannedFolder";
export type { ThreadRepo } from "../../generated/ThreadRepo";
export type { AddThreadRepo } from "../../generated/AddThreadRepo";
export type { StartRepoChoice } from "../../generated/StartRepoChoice";
export type { StartThreadInput } from "../../generated/StartThreadInput";
export type { TaskFolder } from "../../generated/TaskFolder";
import type { RepoRow } from "../../generated/RepoRow";
import type { ReposView } from "../../generated/ReposView";
import type { RepoScan } from "../../generated/RepoScan";
import type { StartThreadInput } from "../../generated/StartThreadInput";
import type { TaskFolder } from "../../generated/TaskFolder";
import type { ThreadRepo } from "../../generated/ThreadRepo";

/** What a publish sends. The service answers the updated `RepoRow`. The host (GitHub) is not a field: the page's provider step only chooses where to publish. */
export interface PublishInput {
  /** `owner/name` on the host. */
  name: string;
  visibility: "private" | "public";
}

/**
 * One Git action on ONE repository of a thread; no action spans repositories. Each is `POST .../git/action`, whose body (`GitAction`)
 * takes an optional `repo_id` (docs/multi-repo.md, "As built: repo_id on Git and file calls"). Merge and review are NOT here: the
 * pull request routes take no `repo_id` yet and act on the primary repository, so a menu for another repository must not offer them.
 * There is no pull (fetch) action: the document defines none.
 */
export type ThreadGitAction =
  | { kind: "commit"; message: string }
  | { kind: "push" }
  | { kind: "pull_request"; title: string };

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
   * Takes one repository out of the thread (`DELETE /api/threads/{id}/repos/{repo_id}`, Person-class, `204`). Never the primary
   * repository. `400` "Commit or discard its changes first" when its worktree holds changes; the message is the person's to read.
   */
  removeThreadRepo(threadId: string, repoId: string): Promise<void>;
  /**
   * One Git action in one repository of the thread. `repoId` is `null` for the project's primary repository: `repo_id` is then left
   * out and the service uses the primary. A repository that is not the thread's is `404 not_found`; an untrusted one is `409 repo_untrusted`.
   */
  threadGit(threadId: string, repoId: string | null, action: ThreadGitAction): Promise<void>;
}
