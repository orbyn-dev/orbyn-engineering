import type { Project, Thread } from "../../../api/types";
import type { ExplorerChange } from "../../../generated/ExplorerChange";
import type { ExplorerEntry } from "../../../generated/ExplorerEntry";
import type { ExplorerReadOnlyReason } from "../../../generated/ExplorerReadOnlyReason";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { PAGE, levelKey, repoLevelKey, type Kept } from "./store";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";

/** The deepest level that still indents; below it a row keeps its place and its tooltip names the folder. */
export const MAX_DEPTH = 8;

type RowKind = "project" | "group" | "thread" | "worktree" | "repo" | "dir" | "file" | "empty" | "error" | "loading" | "more" | "capped";

/**
 * The open multi-repository thread's chosen repository, drawn as its own checkout row. Its tree is read through the
 * thread's file list route with the repository's `repo_id`. A file of the thread's primary repository opens as the thread's
 * own file tab (no repository on it, as the rest of the app opens its files); one of another repository opens as that
 * repository's tab, so its read and write name it.
 */
export interface ThreadCheckout {
  threadId: string;
  /** The `repo_id` the tree reads send; absent is the thread's primary repository, the service's default. */
  readRepoId: string | undefined;
  /** The repository's real id, carried by a file tab of a repository other than the primary; absent for the primary. */
  tabRepoId: string | undefined;
  /** The repository's name, as one line of plain text. */
  name: string;
  /** The thread's own branch in it, the same name in every repository of the thread. */
  branch: string;
}

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
  /** The thread's chosen repository, for the checkout row and the rows below it. */
  checkout?: ThreadCheckout;
  /** Relative to its worktree (or its repository), for a folder or file. */
  path?: string;
  change?: ExplorerChange | null;
  thread?: Thread;
  /** Threads of the project that need the person, on its row. */
  needs?: number;
  /** The worktree this thread runs in. */
  current?: boolean;
  /** The word at the right of the row of a worktree or project that reads only: Read-only, Locked or Missing. */
  badge?: string;
  /** A worktree whose folder is gone: muted, with nothing to unfold. */
  missing?: boolean;
  /** Children of a folder not yet shown, on its "Show more" row. */
  remaining?: number;
  /** The level a "Show more", error or empty row belongs to, or the loading row's. */
  level?: { worktreeId: string; path: string } | { threadId: string; repoId: string | undefined; path: string };
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
  /** The open multi-repository thread's chosen repository, folder by folder. */
  repoLevels: Record<string, Kept<ExplorerTree>>;
  /** The open multi-repository thread's chosen repository; null for an ordinary thread and for a thread in one repository. */
  threadCheckout: ThreadCheckout | null;
  shown: Record<string, number>;
  explorerOpen: Record<string, boolean>;
  needs: Record<string, number>;
}

export const projectKey = (projectId: string) => `p:${projectId}`;
export const groupKey = (projectId: string, group: "threads" | "worktrees") => `g:${projectId}:${group}`;
export const worktreeKey = (projectId: string, worktreeId: string) => `w:${projectId}:${worktreeId}`;
export const folderKey = (projectId: string, worktreeId: string, path: string) => `d:${projectId}:${worktreeId}:${path}`;
/** The checkout row's key, and a folder or file below it; the repository is part of it, so switching starts a fresh row. */
export const checkoutKey = (checkout: ThreadCheckout) => JSON.stringify(["repo", checkout.threadId, checkout.readRepoId ?? ""]);
export const checkoutFolderKey = (checkout: ThreadCheckout, path: string) => JSON.stringify(["repo-dir", checkout.threadId, checkout.readRepoId ?? "", path]);
export const checkoutFileKey = (checkout: ThreadCheckout, path: string) => JSON.stringify(["repo-file", checkout.threadId, checkout.readRepoId ?? "", path]);

/** The worktree a thread runs in, by the threads each worktree lists. */
const worktreeOfThread = (worktrees: readonly ExplorerWorktree[], threadId: string | null) =>
  threadId ? worktrees.find((worktree) => worktree.threads.some((thread) => thread.id === threadId)) : undefined;

export const worktreeLabel = (worktree: ExplorerWorktree) => worktree.branch ?? (worktree.name || worktree.head);

/** Why a worktree is read-only, as far as the person reads it: the service's reason, or that no thread of this one runs there. */
export function readOnlyOf(worktree: ExplorerWorktree, currentThreadId: string | null): boolean {
  // A project's own folder, rooted at itself inside a repository, is never marked read-only.
  if (worktree.is_subfolder) return false;
  if (worktree.read_only) return true;
  return currentThreadId !== null && !worktree.threads.some((thread) => thread.id === currentThreadId);
}

export type ReadOnlyWhy = ExplorerReadOnlyReason | "not_this_thread";

export function readOnlyWhy(worktree: ExplorerWorktree, currentThreadId: string | null): ReadOnlyWhy | null {
  if (worktree.read_only_reason) return worktree.read_only_reason;
  return readOnlyOf(worktree, currentThreadId) ? "not_this_thread" : null;
}

/** The closing line of a folder the service cut: what is listed, of how many it holds. A floor reads "20,000+". */
export function cappedLabel(tree: Pick<ExplorerTree, "entries" | "total" | "total_at_least">): string {
  const count = (n: number) => n.toLocaleString("en-US");
  return `${count(tree.entries.length)} of ${count(tree.total)}${tree.total_at_least ? "+" : ""} shown`;
}

/** Folders first, as the service sends them; ignored files stay out of sight. */
const shownEntries = (entries: readonly ExplorerEntry[]) => entries.filter((entry) => !entry.ignored);

/** A folder level's owner: a worktree of the project, or the open multi-repository thread's chosen repository. */
type LevelOwner = { kind: "worktree"; projectId: string; worktree: ExplorerWorktree } | { kind: "repo"; projectId: string; checkout: ThreadCheckout };

/**
 * The tree as one flat list, only of what is unfolded. A row's place is its depth: the project is 0, its
 * Threads and Worktrees 1, a worktree 2 and what it holds 3 and below.
 */
export function buildRows(input: ExplorerInput): Row[] {
  const { explorerOpen, currentThreadId, currentProjectId } = input;
  const isOpen = (key: string, fallback: boolean) => explorerOpen[key] ?? fallback;
  const rows: Row[] = [];

  const levelRows = (owner: LevelOwner, path: string, depth: number) => {
    const key = owner.kind === "worktree" ? levelKey(owner.projectId, owner.worktree.id, path) : repoLevelKey(owner.projectId, owner.checkout.threadId, owner.checkout.readRepoId, path);
    const level = owner.kind === "worktree" ? input.levels[key] : input.repoLevels[key];
    const at = owner.kind === "worktree"
      ? { worktreeId: owner.worktree.id, path }
      : { threadId: owner.checkout.threadId, repoId: owner.checkout.readRepoId, path };
    const below = owner.kind === "worktree" ? { worktree: owner.worktree } : { checkout: owner.checkout };
    if (!level || (level.loading && !level.data)) {
      rows.push({ key: `loading:${key}`, kind: "loading", depth, projectId: owner.projectId, label: "Loading files", level: at });
      return;
    }
    if (level.error && !level.data) {
      rows.push({ key: `error:${key}`, kind: "error", depth, projectId: owner.projectId, label: "Couldn’t read this folder.", title: level.error.message, retry: "level", level: at });
      return;
    }
    const entries = shownEntries(level.data?.entries ?? []);
    if (entries.length === 0) {
      rows.push({ key: `empty:${key}`, kind: "empty", depth, projectId: owner.projectId, label: "This folder is empty.", level: at });
      return;
    }
    const limit = input.shown[key] ?? PAGE;
    for (const entry of entries.slice(0, limit)) {
      if (entry.kind === "directory") {
        const dirKey = owner.kind === "worktree" ? folderKey(owner.projectId, owner.worktree.id, entry.path) : checkoutFolderKey(owner.checkout, entry.path);
        const open = isOpen(dirKey, false);
        rows.push({ key: dirKey, kind: "dir", depth, projectId: owner.projectId, label: entry.name, path: entry.path, open, ...below });
        if (open) levelRows(owner, entry.path, depth + 1);
      } else {
        const fileKey = owner.kind === "worktree" ? `f:${owner.projectId}:${owner.worktree.id}:${entry.path}` : checkoutFileKey(owner.checkout, entry.path);
        rows.push({
          key: fileKey,
          kind: "file",
          depth,
          projectId: owner.projectId,
          label: entry.name,
          path: entry.path,
          change: entry.change,
          ...below,
        });
      }
    }
    if (entries.length > limit) {
      rows.push({ key: `more:${key}`, kind: "more", depth, projectId: owner.projectId, label: `Show ${entries.length - limit} more`, remaining: entries.length - limit, level: at });
    } else if (level.data?.truncated) {
      // All the service sent is on screen and it held more: say how many, quietly. Nothing is waiting.
      rows.push({ key: `capped:${key}`, kind: "capped", depth, projectId: owner.projectId, label: cappedLabel(level.data), level: at });
    }
  };

  for (const project of input.projects) {
    const here = project.id === currentProjectId;
    const open = isOpen(projectKey(project.id), here);
    const kept = input.worktrees[project.id];
    const list = kept?.data?.worktrees ?? [];
    // Another project is read-only as a whole: no thread of the open one runs in it.
    const otherReadOnly = open && currentThreadId !== null && !here;
    const checkout = here ? input.threadCheckout : null;
    rows.push({
      key: projectKey(project.id),
      kind: "project",
      depth: 0,
      projectId: project.id,
      label: project.name,
      open,
      needs: input.needs[project.id] ?? 0,
      badge: otherReadOnly ? "Read-only" : undefined,
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

    // A folder outside Git, or a project rooted at its own folder inside a repository, is one folder: its files sit
    // directly under the project, with no worktree list and no branch.
    const flat = !!kept?.data && (kept.data.git === false || (list.length === 1 && list[0]!.is_subfolder));
    if (flat) {
      levelRows({ kind: "worktree", projectId: project.id, worktree: list[0]! }, "", 1);
      continue;
    }
    const worktreesOpen = isOpen(groupKey(project.id, "worktrees"), true);
    rows.push({ key: groupKey(project.id, "worktrees"), kind: "group", depth: 1, projectId: project.id, label: "Worktrees", open: worktreesOpen });
    if (!worktreesOpen) continue;
    // The multi-repository thread's chosen repository is the thread's own checkout: it is drawn first, as its own row,
    // and read through the thread's file route rather than the project worktree routes.
    if (checkout) {
      const key = checkoutKey(checkout);
      const checkoutOpen = isOpen(key, true);
      rows.push({ key, kind: "repo", depth: 2, projectId: project.id, label: checkout.branch, title: `${checkout.name} · ${checkout.branch}`, open: checkoutOpen, current: true, checkout });
      if (checkoutOpen) levelRows({ kind: "repo", projectId: project.id, checkout }, "", 3);
    }
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
      // The thread's own checkout of its primary repository is the row above, not one of the project's worktrees.
      if (checkout && worktree.branch === checkout.branch) continue;
      const key = worktreeKey(project.id, worktree.id);
      const missing = worktree.read_only_reason === "missing";
      const locked = worktree.read_only_reason === "locked";
      const wOpen = !missing && isOpen(key, mine?.id === worktree.id);
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
        // The word sits on the project row for another project, on the worktree row otherwise; and only on what is unfolded.
        badge: missing ? "Missing" : wOpen && locked ? "Locked" : wOpen && readOnly && here ? "Read-only" : undefined,
        missing,
      });
      if (wOpen && !missing) levelRows({ kind: "worktree", projectId: project.id, worktree }, "", 3);
    }
  }
  return rows;
}

/** The letter a change is drawn as, and what it says. */
export const CHANGE: Partial<Record<ExplorerChange, { letter: string; label: string }>> = {
  modified: { letter: "M", label: "Modified" },
  added: { letter: "A", label: "Added" },
  deleted: { letter: "D", label: "Deleted" },
  renamed: { letter: "R", label: "Renamed" },
  untracked: { letter: "U", label: "Untracked" },
  conflicted: { letter: "!", label: "Conflicted" },
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
