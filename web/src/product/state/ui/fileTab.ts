import { isFileTab, type FileTabId } from "./types";

/**
 * A file of the thread's checkout. `repoId` names a repository of a multi-repository thread whose worktree holds
 * `path`: another repository's file, whose reads and writes send its `repo_id`. `primary` marks the thread's primary
 * repository (its id is the project's own folder id, or the service's first when that folder is not in the thread): its
 * tab is still the thread's own for reads, writes and failures (the service's absent `repo_id` is the primary) and only
 * names the repository in its chip, its copy path and its line comments.
 */
export type ThreadFileTarget =
  | { kind: "thread"; path: string; repoId?: undefined; primary?: undefined }
  | { kind: "thread"; path: string; repoId: string; primary?: undefined }
  | { kind: "thread"; path: string; repoId: string; primary: true };

/** What a file tab shows: a file of the thread's own checkout, or one read through the left-bar explorer. */
export type FileTarget =
  | ThreadFileTarget
  | { kind: "explorer"; projectId: string; worktreeId: string; path: string }
  /** A worktree with nothing to browse (its folder is missing): the tab says so. */
  | { kind: "worktree"; projectId: string; worktreeId: string }
  /** An issue opened beside the thread: it rides the file tab's identity, order, saving and closing, and reads from the tracker, not a folder. */
  | { kind: "issue"; tracker: string; scope: string; key: string };

/**
 * A file tab's id is its identity (the project and worktree ids and the path), never a display name, so a rename changes
 * nothing. A primary repository's file is its own tab (it names the repository, and reads as the thread's own), so its id
 * carries the repository and `primary`; an ordinary thread's tab stays the one id older sessions saved.
 */
export function fileTabId(target: FileTarget): FileTabId {
  const parts =
    target.kind === "thread"
      ? target.repoId === undefined ? ["t", target.path] : target.primary === true ? ["t", target.path, target.repoId, "primary"] : ["t", target.path, target.repoId]
      : target.kind === "worktree"
        ? ["w", target.projectId, target.worktreeId]
        : target.kind === "issue"
          ? ["i", target.tracker, target.scope, target.key]
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
    // An issue tab saved before the scope was part of its name (two parts) cannot be opened, so it is dropped by the caller.
    if (kind === "i" && rest.length === 3) return { kind: "issue", tracker: rest[0]!, scope: rest[1]!, key: rest[2]! };
    if (kind === "t" && rest.length === 1) return { kind: "thread", path: rest[0]! };
    if (kind === "t" && rest.length === 2 && rest[1] !== "") return { kind: "thread", path: rest[0]!, repoId: rest[1]! };
    // A primary repository's file names its repository too, marked apart from another repository's tab: its reads and
    // writes leave `repo_id` out, and the mark is what tells its tab to still resolve the path as the thread's own.
    if (kind === "t" && rest.length === 3 && rest[1] !== "" && rest[2] === "primary") {
      return { kind: "thread", path: rest[0]!, repoId: rest[1]!, primary: true };
    }
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

/** How an issue's key is written on its tab: a bare number is `#412`, anything else (`ORB-87`) as it is. */
export const issueTabLabel = (key: string): string => {
  const capped = key.length > 24 ? `${key.slice(0, 23)}…` : key;
  return /^\d+$/.test(capped) ? `#${capped}` : capped;
};
