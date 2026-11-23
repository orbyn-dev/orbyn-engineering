import type { RepoRow, ThreadRepo } from "./contract";

export type RepoState = "trusted" | "untrusted" | "not_git";

/**
 * Trust comes first: the service refuses `setup-git` on an untrusted folder (`409 repo_untrusted`), so a folder that is not trusted is
 * `untrusted` whether or not it holds Git. Only a trusted folder with no Git is `not_git`, and only then is Set up Git offered.
 * `add-folder` and `clone` start untrusted; `create` starts trusted with Git.
 */
export function repoState(row: Pick<RepoRow, "is_git" | "trusted">): RepoState {
  if (!row.trusted) return "untrusted";
  return row.is_git ? "trusted" : "not_git";
}

export const STATE_LABEL: Record<RepoState, string> = { trusted: "Trusted", untrusted: "Not trusted", not_git: "Folder" };

/**
 * The folder's Git state where its branch would stand: a folder with no Git says so, rather than naming a branch it has
 * none of. A project row has no `branch`, so the field is optional here.
 */
export function branchText(row: Pick<RepoRow, "is_git" | "trusted"> & { branch?: string | null }): string {
  if (repoState(row) === "not_git") return "Not a Git repository";
  return row.branch ?? "No branch";
}

/** Only a trusted Git folder that is not still cloning can join a thread. An untrusted one never runs anything. */
export function canJoinThread(row: RepoRow): boolean {
  return repoState(row) === "trusted" && row.clone === null;
}

/** The ticks a new thread starts with: every trusted repository, up to the service's limit. */
export function defaultTicks(rows: RepoRow[], limit: number): string[] {
  return rows.filter(canJoinThread).slice(0, Math.max(0, limit)).map((row) => row.id);
}

export const startLabel = (count: number) => (count === 1 ? "Start in 1 repository" : `Start in ${count} repositories`);

/**
 * A clone source, by the list in docs/multi-repo.md ("Clone and path input"): an `https://` URL, an `ssh://` URL or an scp-style
 * `user@host:path`. Text the person typed or pasted is untrusted, and the service checks it again: a source that starts with `-`
 * (Git reads it as an option), `ext::`, `fd::`, `file://`, a local path (so also `owner/name`), a newline or other control
 * character, or a user name and password in the address is refused here and there.
 */
export function checkCloneSource(text: string): { ok: true; source: string } | { ok: false; reason: string } {
  const source = text.trim();
  if (source === "") return { ok: false, reason: "Enter a Git URL." };
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f\s]/.test(source)) return { ok: false, reason: "The address cannot hold spaces or control characters." };
  if (source.startsWith("-")) return { ok: false, reason: "The address cannot start with a dash." };
  const url = /^(https|ssh):\/\/([^/@]*@)?[^/@\s]+\/?[^\s]*$/.exec(source);
  if (url) {
    const userinfo = url[2]?.slice(0, -1) ?? "";
    return userinfo.includes(":") ? { ok: false, reason: "The address cannot hold a user name and password." } : { ok: true, source };
  }
  if (/^[A-Za-z0-9_][\w.-]*@[A-Za-z0-9][\w.-]*:[^\s:-][^\s]*$/.test(source)) return { ok: true, source };
  return { ok: false, reason: "Use a https or ssh address, or user@host:path." };
}

/** The name of a new folder (`create`): letters, digits, dash, underscore and dot, no leading dot or dash, no `..`, no separator and no drive prefix. */
export const isFolderName = (text: string) => /^[A-Za-z0-9_][\w.-]*$/.test(text) && !text.includes("..");

/** The hosts a repository can be published to. Only a ready one can be picked; the rest say Setup Required. The page's own list: the service takes no provider, so the step only shows where it goes. */
export const PUBLISH_PROVIDERS = [
  { id: "github", name: "GitHub", line: "Uses the gh sign-in on this computer", ready: true },
  { id: "gitlab", name: "GitLab", line: "GitLab.com or your own server", ready: false },
  { id: "azure", name: "Azure DevOps", line: "Repos in an organisation", ready: false },
  { id: "forgejo", name: "Forgejo", line: "Your own Forgejo or Codeberg", ready: false },
] as const;

/** `owner/name` for a publish, both parts a plain host name. */
export const isPublishName = (text: string) => /^[A-Za-z0-9][\w.-]*\/[\w.-]+$/.test(text.trim());

/** What the row says about the clone while it runs or after it failed. A running clone is busy with no figure; a failed one says why. */
export function cloneText(row: RepoRow): string | null {
  if (!row.clone) return null;
  if (row.clone.state === "failed") return row.clone.message ? `Clone failed. ${row.clone.message}` : "Clone failed.";
  // The service has no percentage (`percent` is always null): busy only, never a figure or a bar.
  return "Cloning";
}

/** Folder names the service reports, kept to a single line of plain text for a title. */
// eslint-disable-next-line no-control-regex
export const plain = (text: string) => text.replace(/[\u0000-\u001f\u007f]+/g, " ").trim();

/**
 * The changed files of a multi-repository thread, one group per repository in the thread's own order, then the files no repository
 * claims. A file that carries a `repo_id` (docs/multi-repo.md: the Changes list says which repository, with a repo-relative path) is
 * grouped by it, so two repositories can each have a `README.md`. Otherwise a file is claimed by a repository whose `files` list its
 * path. A repository with no file in the list has no group. `[]` means the service did not say which repository a file belongs to,
 * and the list stays flat.
 */
export function groupByRepo<T extends { path: string; repo_id?: string }>(files: readonly T[], repos: readonly ThreadRepo[]): { repo: ThreadRepo | null; files: T[] }[] {
  const byId = files.some((file) => file.repo_id !== undefined);
  if (!byId && !repos.some((repo) => (repo.files?.length ?? 0) > 0)) return [];
  const claimed = new Set<T>();
  const groups = repos.flatMap((repo) => {
    const mine = new Set(repo.files ?? []);
    const found = files.filter((file) => !claimed.has(file) && (file.repo_id !== undefined ? file.repo_id === repo.repo_id : mine.has(file.path)));
    found.forEach((file) => claimed.add(file));
    return found.length > 0 ? [{ repo, files: found }] : [];
  });
  const rest = files.filter((file) => !claimed.has(file));
  return rest.length > 0 ? [...groups, { repo: null, files: rest }] : groups;
}
