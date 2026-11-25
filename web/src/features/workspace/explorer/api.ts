// The one door the left explorer uses to reach the service: four reads, no write.
import { request } from "../../../api/client";
import type { ExplorerFile } from "../../../generated/ExplorerFile";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";
import type { FileEntry } from "../../../generated/FileEntry";

const q = encodeURIComponent;
const base = (projectId: string) => `/api/projects/${q(projectId)}/worktrees`;

/** A project's worktrees. `thread` marks the worktrees another thread runs in as read-only for it; `refresh` reads now. */
export function readWorktrees(projectId: string, options: { thread?: string | null; refresh?: boolean; signal?: AbortSignal } = {}) {
  const query = [options.thread ? `thread=${q(options.thread)}` : "", options.refresh ? "refresh=1" : ""].filter(Boolean).join("&");
  return request<ExplorerWorktrees>("GET", `${base(projectId)}${query ? `?${query}` : ""}`, undefined, options.signal);
}

/** One folder level of a worktree; an empty path is its root. */
export function readTree(projectId: string, worktreeId: string, path: string, options: { refresh?: boolean; signal?: AbortSignal } = {}) {
  const query = `path=${q(path)}${options.refresh ? "&refresh=1" : ""}`;
  return request<ExplorerTree>("GET", `${base(projectId)}/${q(worktreeId)}/tree?${query}`, undefined, options.signal);
}

/** A file of a worktree, read-only. */
export function readExplorerFile(projectId: string, worktreeId: string, path: string, signal?: AbortSignal) {
  return request<ExplorerFile>("GET", `${base(projectId)}/${q(worktreeId)}/file?path=${q(path)}`, undefined, signal);
}

/**
 * One folder level of the open thread's chosen repository (`repoId` absent is its primary repository), read through
 * the thread's own file list route. The answer names each entry but no change letters: `changes` is `unavailable`.
 */
export async function readRepoTree(threadId: string, repoId: string | undefined, path: string, signal?: AbortSignal): Promise<ExplorerTree> {
  const query = `path=${q(path)}${repoId === undefined ? "" : `&repo_id=${q(repoId)}`}`;
  const entries = await request<FileEntry[]>("GET", `/api/threads/${q(threadId)}/files?${query}`, undefined, signal);
  return {
    worktree_id: repoId ?? "",
    path,
    entries: entries.map((entry) => ({
      name: entry.name,
      path: entry.path,
      kind: entry.is_dir ? "directory" : "file",
      size: Number(entry.size),
      ignored: entry.ignored,
      change: null,
      has_changes: false,
    })),
    truncated: false,
    total: entries.length,
    total_at_least: false,
    changes: "unavailable",
    read_only: false,
  };
}
