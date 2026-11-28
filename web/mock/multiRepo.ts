/**
 * The dev mock's multi-repository sample (docs/multi-repo.md). The plain mock (mock/api.ts) and the preview mock
 * (mock/repos.ts) both answer for this one thread, so the Repos chip and its dialog can be reached in a browser.
 * Browser-safe on purpose: it reads no environment, so the preview mock can import it too.
 */
import type { Project } from "../src/api/types.ts";
import type { RepoRow } from "../src/generated/RepoRow.ts";
import type { ReposView } from "../src/generated/ReposView.ts";
import type { TaskFolder } from "../src/generated/TaskFolder.ts";
import type { ThreadRepo } from "../src/generated/ThreadRepo.ts";

/** The fixture thread (mock/fixtures.ts) that works in two repositories. */
export const multiRepoThreadId = "t-multi-repo";
/** The one branch every repository's worktree is cut on. */
export const multiRepoBranch = "orbyn/1006-repos";

const threadRepos: ThreadRepo[] = [
  { repo_id: "p-app", name: "orbyn-engineering-app", branch: multiRepoBranch, added: 4, deleted: 1, trusted: true, files: ["auth/redirect.ts", "auth/redirect.test.ts"] },
  { repo_id: "orbyn-planner", name: "orbyn-planner", branch: multiRepoBranch, added: 2, deleted: 0, trusted: true, files: ["calendar/feed.ts"] },
];

/** The repositories the sample thread works in, or `null` (a `404`) for any other thread. */
export function multiRepoRepos(threadId: string): ThreadRepo[] | null {
  return threadId === multiRepoThreadId ? structuredClone(threadRepos) : null;
}

/** The sample thread's task folder, or `null` (a `404`) for any other thread. */
export function multiRepoTaskFolder(threadId: string): TaskFolder | null {
  const repos = multiRepoRepos(threadId);
  return repos === null ? null : { root: "~/Library/Application Support/Orbyn/threads/t-multi-repo", branch: multiRepoBranch, repos, max_per_thread: 8 };
}

/**
 * A project's repositories as the service answers them: its own folder first (`id` equal to the project id),
 * and p-app beside it the two more the sample thread works in or can add. Other projects answer one row.
 */
export function projectReposView(project: Project): ReposView {
  const primary: RepoRow = {
    id: project.id, name: project.name, path: project.path, is_git: project.is_git, trusted: project.trusted,
    branch: project.is_git ? "main" : null, branches: project.is_git ? ["main"] : [], has_remote: false, primary: true, clone: null,
  };
  const extra: RepoRow[] = project.id === "p-app" ? [
    { id: "orbyn-planner", name: "orbyn-planner", path: "/Users/dev/code/orbyn-planner", is_git: true, trusted: true, branch: "feat/calendar", branches: ["feat/calendar", "main"], has_remote: false, primary: false, clone: null },
    { id: "orbyn-engine", name: "orbyn-engine", path: "/Users/dev/code/orbyn-engine", is_git: true, trusted: true, branch: "main", branches: ["main"], has_remote: true, primary: false, clone: null },
  ] : [];
  return { repositories: [primary, ...extra], clone_directory: "/Users/dev/Documents/Github", max_per_thread: 8 };
}
