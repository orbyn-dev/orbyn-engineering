import type { RepoRow, ThreadRepo } from "./contract";

export type RepoState = "trusted" | "untrusted" | "not_git";

/** A folder that is not Git has no trust to decide until Git is set up. */
export function repoState(row: Pick<RepoRow, "is_git" | "trusted">): RepoState {
  if (!row.is_git) return "not_git";
  return row.trusted ? "trusted" : "untrusted";
}

export const STATE_LABEL: Record<RepoState, string> = { trusted: "Trusted", untrusted: "Not trusted", not_git: "Not a Git folder" };

/** Only a trusted Git folder that is not still cloning can join a thread. An untrusted one never runs anything. */
export function canJoinThread(row: RepoRow): boolean {
  return repoState(row) === "trusted" && row.clone === null;
}

/** The ticks a new thread starts with: every trusted repository, up to the service's limit. */
export function defaultTicks(rows: RepoRow[], limit: number): string[] {
  return rows.filter(canJoinThread).slice(0, Math.max(0, limit)).map((row) => row.id);
}

export const startLabel = (count: number) => (count === 1 ? "Start in 1 repository" : `Start in ${count} repositories`);

/** A Git address or `owner/name`. Text the person typed or pasted is untrusted: nothing that Git could read as an option or that holds a control character passes. */
export function checkCloneSource(text: string): { ok: true; source: string } | { ok: false; reason: string } {
  const source = text.trim();
  if (source === "") return { ok: false, reason: "Enter a Git URL or owner/name." };
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f\s]/.test(source)) return { ok: false, reason: "The address cannot hold spaces or control characters." };
  if (source.startsWith("-")) return { ok: false, reason: "The address cannot start with a dash." };
  const ownerName = /^[\w.-]+\/[\w.-]+$/.test(source) && !/^\.{1,2}\//.test(source);
  const url = /^(https:\/\/|ssh:\/\/|git@[\w.-]+:)[^\s]+$/.test(source);
  return ownerName || url ? { ok: true, source } : { ok: false, reason: "Use a https or ssh address, git@host:path, or owner/name." };
}

/** `owner/name` for a publish, both parts a plain host name. */
export const isPublishName = (text: string) => /^[A-Za-z0-9][\w.-]*\/[\w.-]+$/.test(text.trim());

/** What the row says about the clone while it runs or after it failed. */
export function cloneText(row: RepoRow): string | null {
  if (!row.clone) return null;
  if (row.clone.state === "failed") return row.clone.message ? `Clone failed. ${row.clone.message}` : "Clone failed.";
  return row.clone.percent === null ? "Cloning" : `Cloning ${Math.round(row.clone.percent)}%`;
}

/** Folder names the service reports, kept to a single line of plain text for a title. */
// eslint-disable-next-line no-control-regex
export const plain = (text: string) => text.replace(/[\u0000-\u001f\u007f]+/g, " ").trim();

/**
 * The changed files of a multi-repository thread, one group per repository in the thread's own order, then the files no repository
 * claims. A repository with no file in the list has no group. `[]` means the service did not say which repository a file belongs to,
 * and the list stays flat.
 */
export function groupByRepo<T extends { path: string }>(files: readonly T[], repos: readonly ThreadRepo[]): { repo: ThreadRepo | null; files: T[] }[] {
  if (!repos.some((repo) => (repo.files?.length ?? 0) > 0)) return [];
  const claimed = new Set<string>();
  const groups = repos.flatMap((repo) => {
    const mine = new Set(repo.files ?? []);
    const found = files.filter((file) => mine.has(file.path) && !claimed.has(file.path));
    found.forEach((file) => claimed.add(file.path));
    return found.length > 0 ? [{ repo, files: found }] : [];
  });
  const rest = files.filter((file) => !claimed.has(file.path));
  return rest.length > 0 ? [...groups, { repo: null, files: rest }] : groups;
}
