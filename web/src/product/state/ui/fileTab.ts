import { isFileTab, type FileTabId } from "./types";

/** What a file tab shows: a file of the thread's own checkout, or one read through the left-bar explorer. */
export type FileTarget =
  | { kind: "thread"; path: string }
  | { kind: "explorer"; projectId: string; worktreeId: string; path: string }
  /** A worktree with nothing to browse (its folder is missing): the tab says so. */
  | { kind: "worktree"; projectId: string; worktreeId: string };

/** A file tab's id is its identity (the project and worktree ids and the path), never a display name, so a rename changes nothing. */
export function fileTabId(target: FileTarget): FileTabId {
  const parts =
    target.kind === "thread"
      ? ["t", target.path]
      : target.kind === "worktree"
        ? ["w", target.projectId, target.worktreeId]
        : ["x", target.projectId, target.worktreeId, target.path];
  return `file:${encodeURIComponent(JSON.stringify(parts))}`;
}

/** The target an id names, or null for an id that is not one of ours. */
export function parseFileTab(id: string): FileTarget | null {
  if (!isFileTab(id)) return null;
  try {
    const parts: unknown = JSON.parse(decodeURIComponent(id.slice(5)));
    if (!Array.isArray(parts) || !parts.every((part) => typeof part === "string")) return null;
    const [kind, ...rest] = parts as string[];
    if (kind === "w" && rest.length === 2) return { kind: "worktree", projectId: rest[0]!, worktreeId: rest[1]! };
    if (kind === "t" && rest.length === 1) return { kind: "thread", path: rest[0]! };
    // A tab saved before names left the id carries two more parts, which are ignored.
    if (kind === "x" && (rest.length === 3 || rest.length === 5)) {
      const [projectId, worktreeId, path] = rest as [string, string, string];
      return { kind: "explorer", projectId, worktreeId, path };
    }
  } catch {
    // A damaged saved id is dropped by the caller.
  }
  return null;
}

/** The last part of a path, which names the tab. */
export const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;

/** The id a saved one names today: an id from before names left it becomes the same tab as a new one. */
export function canonicalFileTab(id: unknown): unknown {
  if (typeof id !== "string") return id;
  const target = parseFileTab(id);
  return target ? fileTabId(target) : id;
}
