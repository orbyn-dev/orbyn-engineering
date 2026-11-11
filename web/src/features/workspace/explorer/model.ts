import type { Project, Thread } from "../../../api/types";
import type { ExplorerChange } from "../../../generated/ExplorerChange";
import type { ExplorerEntry } from "../../../generated/ExplorerEntry";
import type { ExplorerReadOnlyReason } from "../../../generated/ExplorerReadOnlyReason";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { PAGE, levelKey, type Kept } from "./store";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";

/** The deepest level that still indents; below it a row keeps its place and its tooltip names the folder. */
export const MAX_DEPTH = 8;

type RowKind = "project" | "group" | "thread" | "worktree" | "dir" | "file" | "empty" | "error" | "loading" | "more";

/** One line of the tree. Only what a row draws or acts on is here; data stays in the store. */
export interface Row {
  key: string;
  kind: RowKind;
  depth: number;
  projectId: string;
  label: string;
  open?: boolean;
  /** Where it lives, for a row of a worktree. */
  worktree?: ExplorerWorktree;
  /** Relative to its worktree, for a folder or file. */
  path?: string;
  change?: ExplorerChange | null;
  thread?: Thread;
  /** Threads of the project that need the person, on its row. */
  needs?: number;
  /** The worktree this thread runs in. */
  current?: boolean;
  /** "Read-only", on the row of what is shown read-only. */
  readOnly?: boolean;
  /** Children of a folder not yet shown, on its "Show more" row. */
  remaining?: number;
  /** The level a "Show more", error or empty row belongs to, or the loading row's. */
  level?: { worktreeId: string; path: string };
  /** What a retry reads again. */
  retry?: "worktrees" | "level";
  /** Why the row is as it is, for its tooltip. */
  title?: string;
}

export interface ExplorerInput {
  projects: readonly Project[];
  /** Threads that belong in the Threads group of each project, in the order shown. */
  threads: Record<string, Thread[]>;
  currentThreadId: string | null;
  currentProjectId: string | null;
  worktrees: Record<string, Kept<ExplorerWorktrees>>;
  levels: Record<string, Kept<ExplorerTree>>;
  shown: Record<string, number>;
  explorerOpen: Record<string, boolean>;
  needs: Record<string, number>;
}

export const projectKey = (projectId: string) => `p:${projectId}`;
export const groupKey = (projectId: string, group: "threads" | "worktrees") => `g:${projectId}:${group}`;
export const worktreeKey = (projectId: string, worktreeId: string) => `w:${projectId}:${worktreeId}`;
export const folderKey = (projectId: string, worktreeId: string, path: string) => `d:${projectId}:${worktreeId}:${path}`;

/** The worktree a thread runs in, by the threads each worktree lists. */
const worktreeOfThread = (worktrees: readonly ExplorerWorktree[], threadId: string | null) =>
  threadId ? worktrees.find((worktree) => worktree.threads.some((thread) => thread.id === threadId)) : undefined;

export const worktreeLabel = (worktree: ExplorerWorktree) => worktree.branch ?? (worktree.name || worktree.head);

/** Why a worktree is read-only, as far as the person reads it: the service's reason, or that no thread of this one runs there. */
export function readOnlyOf(worktree: ExplorerWorktree, currentThreadId: string | null): boolean {
  if (worktree.read_only) return true;
  return currentThreadId !== null && !worktree.threads.some((thread) => thread.id === currentThreadId);
}

export type ReadOnlyWhy = ExplorerReadOnlyReason | "not_this_thread";

export function readOnlyWhy(worktree: ExplorerWorktree, currentThreadId: string | null): ReadOnlyWhy | null {
  if (worktree.read_only_reason) return worktree.read_only_reason;
  return readOnlyOf(worktree, currentThreadId) ? "not_this_thread" : null;
}

/** Folders first, as the service sends them; ignored files stay out of sight. */
const shownEntries = (entries: readonly ExplorerEntry[]) => entries.filter((entry) => !entry.ignored);

/**
 * The tree as one flat list, only of what is unfolded. A row's place is its depth: the project is 0, its
 * Threads and Worktrees 1, a worktree 2 and what it holds 3 and below.
 */
export function buildRows(input: ExplorerInput): Row[] {
  const { explorerOpen, currentThreadId, currentProjectId } = input;
  const isOpen = (key: string, fallback: boolean) => explorerOpen[key] ?? fallback;
  const rows: Row[] = [];

  const levelRows = (projectId: string, worktree: ExplorerWorktree, path: string, depth: number) => {
    const key = levelKey(projectId, worktree.id, path);
    const level = input.levels[key];
    const at = { worktreeId: worktree.id, path };
    if (!level || (level.loading && !level.data)) {
      rows.push({ key: `loading:${key}`, kind: "loading", depth, projectId, label: "Loading files", level: at });
      return;
    }
    if (level.error && !level.data) {
      rows.push({ key: `error:${key}`, kind: "error", depth, projectId, label: "Couldn’t read this folder.", title: level.error.message, retry: "level", level: at });
      return;
    }
    const entries = shownEntries(level.data?.entries ?? []);
    if (entries.length === 0) {
      rows.push({ key: `empty:${key}`, kind: "empty", depth, projectId, label: "This folder is empty.", level: at });
      return;
    }
    const limit = input.shown[key] ?? PAGE;
    for (const entry of entries.slice(0, limit)) {
      if (entry.kind === "directory") {
        const dirKey = folderKey(projectId, worktree.id, entry.path);
        const open = isOpen(dirKey, false);
        rows.push({ key: dirKey, kind: "dir", depth, projectId, label: entry.name, path: entry.path, open, worktree });
        if (open) levelRows(projectId, worktree, entry.path, depth + 1);
      } else {
        rows.push({
          key: `f:${projectId}:${worktree.id}:${entry.path}`,
          kind: "file",
          depth,
          projectId,
          label: entry.name,
          path: entry.path,
          change: entry.change,
          worktree,
        });
      }
    }
    if (entries.length > limit) {
      rows.push({ key: `more:${key}`, kind: "more", depth, projectId, label: `Show ${entries.length - limit} more`, remaining: entries.length - limit, level: at });
    }
  };

  for (const project of input.projects) {
    const here = project.id === currentProjectId;
    const open = isOpen(projectKey(project.id), here);
    const kept = input.worktrees[project.id];
    const list = kept?.data?.worktrees ?? [];
    // Another project is read-only as a whole: no thread of the open one runs in it.
    const otherReadOnly = open && currentThreadId !== null && !here;
    rows.push({
      key: projectKey(project.id),
      kind: "project",
      depth: 0,
      projectId: project.id,
      label: project.name,
      open,
      needs: input.needs[project.id] ?? 0,
      readOnly: otherReadOnly,
    });
    if (!open) continue;

    const threads = input.threads[project.id] ?? [];
    if (threads.length > 0) {
      const threadsOpen = isOpen(groupKey(project.id, "threads"), true);
      rows.push({ key: groupKey(project.id, "threads"), kind: "group", depth: 1, projectId: project.id, label: "Threads", open: threadsOpen });
      if (threadsOpen) {
        for (const thread of threads) rows.push({ key: `t:${thread.id}`, kind: "thread", depth: 2, projectId: project.id, label: thread.title, thread });
      }
    }

    const worktreesOpen = isOpen(groupKey(project.id, "worktrees"), true);
    rows.push({ key: groupKey(project.id, "worktrees"), kind: "group", depth: 1, projectId: project.id, label: "Worktrees", open: worktreesOpen });
    if (!worktreesOpen) continue;
    if (!kept || (kept.loading && !kept.data)) {
      rows.push({ key: `loading:w:${project.id}`, kind: "loading", depth: 2, projectId: project.id, label: "Loading worktrees" });
      continue;
    }
    if (kept.error && !kept.data) {
      rows.push({ key: `error:w:${project.id}`, kind: "error", depth: 2, projectId: project.id, label: "Couldn’t read this folder.", title: kept.error.message, retry: "worktrees" });
      continue;
    }
    const mine = worktreeOfThread(list, currentThreadId);
    for (const worktree of list) {
      const key = worktreeKey(project.id, worktree.id);
      const wOpen = isOpen(key, mine?.id === worktree.id);
      const readOnly = readOnlyOf(worktree, currentThreadId);
      rows.push({
        key,
        kind: "worktree",
        depth: 2,
        projectId: project.id,
        label: worktreeLabel(worktree),
        title: worktree.path,
        open: wOpen,
        worktree,
        current: mine?.id === worktree.id,
        // The label sits on the project row for another project, on the worktree row otherwise; and only on what is unfolded.
        readOnly: wOpen && readOnly && here,
      });
      if (wOpen) levelRows(project.id, worktree, "", 3);
    }
  }
  return rows;
}

/** The letter a change is drawn as, and what it says. */
export const CHANGE: Record<ExplorerChange, { letter: string; label: string }> = {
  modified: { letter: "M", label: "Modified" },
  added: { letter: "A", label: "Added" },
  deleted: { letter: "D", label: "Deleted" },
  renamed: { letter: "R", label: "Renamed" },
  untracked: { letter: "U", label: "Untracked" },
  conflicted: { letter: "!", label: "Conflict" },
};

/** A folder path as it is compared: one separator, no trailing one, macOS's /private prefix of /var, /tmp and /etc dropped, and no case on a drive. */
const clean = (path: string) => {
  const slashed = path.replace(/\\/g, "/").replace(/\/+$/, "").replace(/^\/private(?=\/(var|tmp|etc)(\/|$))/, "");
  return /^[A-Za-z]:/.test(slashed) ? slashed.toLowerCase() : slashed;
};

/** Whether two paths name the same folder, ignoring the separator and a trailing one. */
export const samePath = (a: string, b: string) => clean(a) === clean(b);

/**
 * Where `folder` is below `root`: "" for the root itself, a relative `/` path inside it, or null when it is
 * not inside. A thread's folder can be a subfolder of its worktree, since the service roots a worktree at the Git root.
 */
export function insidePath(root: string, folder: string): string | null {
  const [r, f] = [clean(root), clean(folder)];
  if (f === r) return "";
  return f.startsWith(`${r}/`) ? f.slice(r.length + 1) : null;
}

/**
 * A file of a worktree (`path`, relative to the worktree) as the thread that works in `cwd` names it: relative to
 * `cwd`. Null when the thread's folder is not in that worktree, or the file is outside the thread's folder, so the
 * thread's own file routes cannot reach it.
 */
export function threadPathOf(worktreePath: string, cwd: string, path: string): string | null {
  const base = insidePath(worktreePath, cwd);
  if (base === null) return null;
  if (base === "") return path;
  return path.startsWith(`${base}/`) ? path.slice(base.length + 1) : null;
}

/** The last parts of a folder path, for a deep row's tooltip. */
export const crumb = (path: string) => path.split("/").filter(Boolean).join(" › ");
