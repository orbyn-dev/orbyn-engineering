/**
 * The mock's left explorer: the same three reads as the service (`/api/projects/{id}/worktrees`, `.../tree`,
 * `.../file`), with a few worktrees and one folder large enough to page. Every worktree has its own copy of
 * the files, so a read in another worktree shows that worktree's version.
 */
import type { ExplorerChange } from "../src/generated/ExplorerChange.ts";
import type { ExplorerEntry } from "../src/generated/ExplorerEntry.ts";
import type { ExplorerTree } from "../src/generated/ExplorerTree.ts";
import type { ExplorerWorktree } from "../src/generated/ExplorerWorktree.ts";
import type { ExplorerWorktrees } from "../src/generated/ExplorerWorktrees.ts";
import type { Project, Thread } from "../src/api/types.ts";
import type { MockResponse } from "./api.ts";

const err = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });

/** Files of the app project: a path and the change Git reports for it. A path ending in `/` is a folder with nothing in it. */
const APP_FILES: [string, ExplorerChange | null][] = [
  ["README.md", "modified"],
  ["Cargo.toml", null],
  ["docs/guide.md", null],
  ["empty/", null],
  ["src/main.rs", "modified"],
  ["src/lib.rs", null],
  ["src/auth/redirect.ts", "modified"],
  ["src/auth/redirect.test.ts", "modified"],
  ["src/auth/session.ts", null],
  ...Array.from({ length: 260 }, (_, at): [string, ExplorerChange | null] => [`src/generated/file-${String(at).padStart(3, "0")}.ts`, null]),
];
const SITE_FILES: [string, ExplorerChange | null][] = [["index.html", null], ["styles/site.css", "added"]];

const sorted = (entries: ExplorerEntry[]) =>
  entries.sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "directory" ? -1 : 1));

function level(files: [string, ExplorerChange | null][], path: string): ExplorerEntry[] {
  const base = path ? `${path}/` : "";
  const seen = new Map<string, ExplorerEntry>();
  for (const [file, change] of files) {
    if (!file.startsWith(base)) continue;
    const [first, ...rest] = file.slice(base.length).split("/");
    if (!first) continue;
    const dir = rest.length > 0;
    const entryPath = `${base}${first}`;
    const held = seen.get(first);
    if (!held) seen.set(first, { name: first, path: entryPath, kind: dir ? "directory" : "file", size: dir ? null : 120, ignored: false, change: dir ? null : change, has_changes: dir && change !== null });
    else if (dir && change !== null) held.has_changes = true;
  }
  return sorted([...seen.values()].filter((entry) => entry.name !== ""));
}

export function createExplorerMock(getProjects: () => Project[], getThreads: () => Thread[]) {
  const worktreesOf = (project: Project): ExplorerWorktrees => {
    const live = (predicate: (thread: Thread) => boolean) =>
      getThreads().filter((thread) => thread.project_id === project.id && predicate(thread)).map((thread) => ({ id: thread.id, title: thread.title, running: thread.active_run !== null }));
    const base = { detached: false, head: "a1b2c3d", read_only: false, read_only_reason: null, dirty: null } as const;
    if (!project.is_git) {
      return { project_id: project.id, git: false, worktrees: [{ ...base, id: `${project.id}-folder`, path: project.path, name: project.name, branch: null, is_main: true, threads: live(() => true) }] };
    }
    const main: ExplorerWorktree = { ...base, id: `${project.id}-main`, path: project.path, name: project.name, branch: "main", is_main: true, threads: live((thread) => thread.worktree === null) };
    if (project.id !== "p-app") return { project_id: project.id, git: true, worktrees: [main] };
    const wt = "/Users/dev/code/orbyn-engineering-app-wt";
    return {
      project_id: project.id,
      git: true,
      worktrees: [
        { ...main, dirty: { changed: 4, untracked: 0, conflicted: 0 } },
        { ...base, id: "p-app-approval", path: `${wt}/approval-timeout`, name: "approval-timeout", branch: "approval-timeout", is_main: false, threads: live((thread) => thread.worktree?.branch === "approval-timeout") },
        { ...base, id: "p-app-locked", path: `${wt}/locked`, name: "locked", branch: "feat/locked", is_main: false, read_only: true, read_only_reason: "locked", threads: [] },
        { ...base, id: "p-app-gone", path: `${wt}/gone`, name: "gone", branch: "feat/gone", is_main: false, read_only: true, read_only_reason: "missing", threads: [] },
      ],
    };
  };

  const filesOf = (projectId: string) => (projectId === "p-app" ? APP_FILES : SITE_FILES);

  return function explorerRoute(method: string, seg: string[], query: URLSearchParams): MockResponse | null {
    if (method !== "GET" || seg[1] !== "projects" || seg[3] !== "worktrees") return null;
    const project = getProjects().find((one) => one.id === seg[2]);
    if (!project) return err(404, "not_found", "Project not found.");
    const all = worktreesOf(project);
    if (!seg[4]) return { status: 200, body: all };
    const worktree = all.worktrees.find((one) => one.id === seg[4]);
    if (!worktree) return err(404, "not_found", "That worktree isn't in this project.");
    if (worktree.read_only_reason === "missing") return err(404, "not_found", "That worktree's folder is gone.");
    const path = query.get("path") ?? "";
    if (seg[5] === "tree") {
      const body: ExplorerTree = { worktree_id: worktree.id, path, entries: level(filesOf(project.id), path), truncated: false, changes: "ready", read_only: worktree.read_only };
      return { status: 200, body };
    }
    if (seg[5] === "file") {
      const found = filesOf(project.id).find(([file]) => file === path);
      if (!found) return err(404, "not_found", "File not found.");
      const content = `// ${path}\n// ${worktree.branch ?? worktree.name}\nexport const answer = 42\n`;
      return { status: 200, body: { worktree_id: worktree.id, path, kind: "text", content, size: content.length, revision: "mock", change: found[1], read_only: worktree.read_only } };
    }
    return null;
  };
}
