import { isFileTab, type FileTabId } from "./types";

/** What a file tab shows: a file of the thread's own checkout, or one read through the left-bar explorer. */
export type FileTarget =
  | { kind: "thread"; path: string }
  | { kind: "explorer"; projectId: string; worktreeId: string; path: string; project: string; worktree: string };

/** A file tab's id is its target, so a tab saved for a thread opens the same file again. */
export function fileTabId(target: FileTarget): FileTabId {
  const parts =
    target.kind === "thread"
      ? ["t", target.path]
      : ["x", target.projectId, target.worktreeId, target.path, target.project, target.worktree];
  return `file:${encodeURIComponent(JSON.stringify(parts))}`;
}

/** The target an id names, or null for an id that is not one of ours. */
export function parseFileTab(id: string): FileTarget | null {
  if (!isFileTab(id)) return null;
  try {
    const parts: unknown = JSON.parse(decodeURIComponent(id.slice(5)));
    if (!Array.isArray(parts) || !parts.every((part) => typeof part === "string")) return null;
    const [kind, ...rest] = parts as string[];
    if (kind === "t" && rest.length === 1) return { kind: "thread", path: rest[0]! };
    if (kind === "x" && rest.length === 5) {
      const [projectId, worktreeId, path, project, worktree] = rest as [string, string, string, string, string];
      return { kind: "explorer", projectId, worktreeId, path, project, worktree };
    }
  } catch {
    // A damaged saved id is dropped by the caller.
  }
  return null;
}

/** The last part of a path, which names the tab. */
export const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
