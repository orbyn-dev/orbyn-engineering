// The one door the left explorer uses to reach the service: three reads, no write.
import { request } from "../../../api/client";
import type { ExplorerFile } from "../../../generated/ExplorerFile";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";

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
