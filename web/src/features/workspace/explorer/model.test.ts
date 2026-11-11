import { describe, expect, it } from "vitest";
import type { Project, Thread } from "../../../api/types";
import type { ExplorerEntry } from "../../../generated/ExplorerEntry";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { ApiFailure } from "../../../api/client";
import { buildRows, crumb, folderKey, groupKey, insidePath, projectKey, readOnlyOf, readOnlyWhy, samePath, threadPathOf, worktreeKey, type ExplorerInput } from "./model";
import { PAGE, levelKey, type Kept } from "./store";

const project = { id: "p", name: "app" } as Project;
const other = { id: "q", name: "other" } as Project;
const worktree = (over: Partial<ExplorerWorktree>): ExplorerWorktree => ({
  id: "w", path: "/r", name: "r", branch: "main", detached: false, head: "abc", is_main: true,
  read_only: false, read_only_reason: null, threads: [], dirty: null, is_subfolder: false, ...over,
});
const entry = (name: string, over: Partial<ExplorerEntry> = {}): ExplorerEntry => ({
  name, path: name, kind: "file", size: 1, ignored: false, change: null, has_changes: false, ...over,
});
const kept = <T,>(data: T): Kept<T> => ({ data, loading: false, error: null });
const level = (entries: ExplorerEntry[]): Kept<ExplorerTree> =>
  kept({ worktree_id: "w", path: "", entries, truncated: false, changes: "ready", read_only: false });
const thread = (id: string) => ({ id, title: id, project_id: "p" }) as Thread;

const base = (over: Partial<ExplorerInput> = {}): ExplorerInput => ({
  projects: [project, other], threads: {}, currentThreadId: "t", currentProjectId: "p",
  worktrees: { p: kept({ project_id: "p", git: true, worktrees: [worktree({ threads: [{ id: "t", title: "t", running: false }] }), worktree({ id: "x", branch: "feat/x", is_main: false })] }) },
  levels: {}, shown: {}, explorerOpen: {}, needs: {}, ...over,
});
const keys = (rows: ReturnType<typeof buildRows>) => rows.map((row) => `${row.kind}:${row.depth}:${row.label}`);

describe("explorer rows", () => {
  it("unfolds the open thread's project and worktree, and folds everything else", () => {
    const rows = buildRows(base({ threads: { p: [thread("t")] } }));
    expect(keys(rows)).toEqual([
      "project:0:app", "group:1:Threads", "thread:2:t", "group:1:Worktrees",
      "worktree:2:main", "loading:3:Loading files", "worktree:2:feat/x", "project:0:other",
    ]);
    expect(rows.find((row) => row.kind === "worktree" && row.current)?.label).toBe("main");
  });

  it("asks for a folder only once it is unfolded: a folded folder has no loading row", () => {
    const entries = [entry("src", { kind: "directory" }), entry("a.ts")];
    const input = base({ levels: { [levelKey("p", "w", "")]: level(entries) } });
    expect(keys(buildRows(input))).toContain("dir:3:src");
    expect(buildRows(input).some((row) => row.level?.path === "src")).toBe(false);
    const open = base({ levels: input.levels, explorerOpen: { [folderKey("p", "w", "src")]: true } });
    expect(buildRows(open).find((row) => row.kind === "loading")?.level).toEqual({ worktreeId: "w", path: "src" });
  });

  it("lists folders and files as the service sent them, hides ignored files, and says an empty folder is empty", () => {
    const rows = buildRows(base({ levels: { [levelKey("p", "w", "")]: level([entry("a.ts"), entry("dist", { ignored: true, kind: "directory" })]) } }));
    expect(keys(rows)).toContain("file:3:a.ts");
    expect(keys(rows).some((row) => row.endsWith(":dist"))).toBe(false);
    const empty = buildRows(base({ levels: { [levelKey("p", "w", "")]: level([]) } }));
    expect(keys(empty)).toContain("empty:3:This folder is empty.");
  });

  it("puts one error row where a folder's files would be, with the reason for its tooltip", () => {
    const failure = new ApiFailure("forbidden", "Permission denied.", 403);
    const rows = buildRows(base({ levels: { [levelKey("p", "w", "")]: { data: null, loading: false, error: failure } } }));
    const error = rows.find((row) => row.kind === "error");
    expect(error).toMatchObject({ label: "Couldn’t read this folder.", title: "Permission denied.", retry: "level", depth: 3 });
  });

  it("shows the first 200 children of a folder and a row for the rest", () => {
    const many = Array.from({ length: PAGE + 12 }, (_, at) => entry(`f${String(at).padStart(3, "0")}`));
    const input = base({ levels: { [levelKey("p", "w", "")]: level(many) } });
    const rows = buildRows(input);
    expect(rows.filter((row) => row.kind === "file")).toHaveLength(PAGE);
    expect(rows.find((row) => row.kind === "more")).toMatchObject({ label: "Show 12 more", remaining: 12 });
    const next = buildRows({ ...input, shown: { [levelKey("p", "w", "")]: PAGE * 2 } });
    expect(next.filter((row) => row.kind === "file")).toHaveLength(PAGE + 12);
    expect(next.some((row) => row.kind === "more")).toBe(false);
  });

  it("marks what is read-only: the worktree row in its own project, the project row for another", () => {
    const rows = buildRows(base({
      explorerOpen: { [worktreeKey("p", "x")]: true, [projectKey("q")]: true, [groupKey("q", "worktrees")]: true },
      levels: { [levelKey("p", "x", "")]: level([entry("a.ts")]) },
    }));
    const labelled = rows.filter((row) => row.badge).map((row) => `${row.kind}:${row.label}:${row.badge}`);
    expect(labelled).toEqual(["worktree:feat/x:Read-only", "project:other:Read-only"]);
    // The thread's own worktree is not.
    expect(rows.find((row) => row.kind === "worktree" && row.label === "main")?.badge).toBeUndefined();
  });

  it("says Locked on an unfolded locked worktree, and shows a missing one muted with nothing to unfold", () => {
    const worktrees = { p: kept({ project_id: "p", git: true, worktrees: [
      worktree({ id: "l", branch: "feat/l", is_main: false, read_only: true, read_only_reason: "locked" }),
      worktree({ id: "m", branch: "feat/m", is_main: false, read_only: true, read_only_reason: "missing" }),
    ] }) };
    const rows = buildRows(base({ worktrees, explorerOpen: { [worktreeKey("p", "l")]: true, [worktreeKey("p", "m")]: true }, levels: { [levelKey("p", "l", "")]: level([entry("a.ts")]) } }));
    expect(rows.find((row) => row.label === "feat/l")?.badge).toBe("Locked");
    const missing = rows.find((row) => row.label === "feat/m")!;
    expect(missing).toMatchObject({ badge: "Missing", missing: true, open: false });
    // Nothing is read below a missing worktree, whatever was saved as unfolded.
    expect(rows.filter((row) => row.level?.worktreeId === "m")).toEqual([]);
  });

  it("lists a folder outside Git, or a project rooted at its own folder, as one folder: files directly under the project", () => {
    const flat = (over: Partial<ExplorerWorktree>, git: boolean) =>
      buildRows(base({
        currentProjectId: "p",
        worktrees: { p: kept({ project_id: "p", git, worktrees: [worktree({ branch: null, threads: [{ id: "t", title: "", running: false }], ...over })] }) },
        levels: { [levelKey("p", "w", "")]: level([entry("notes.md")]) },
      }));
    for (const rows of [flat({}, false), flat({ is_subfolder: true }, true)]) {
      expect(keys(rows).slice(0, 2)).toEqual(["project:0:app", "file:1:notes.md"]);
      expect(rows.some((row) => row.kind === "worktree" || row.label === "Worktrees")).toBe(false);
    }
  });

  it("never reads a project's own folder inside a repository as read-only", () => {
    expect(readOnlyOf(worktree({ is_subfolder: true, read_only: true, read_only_reason: "locked" }), "t")).toBe(false);
  });

  it("indents a level 13px and leaves a deep row's place to its depth", () => {
    expect(crumb("a/b/c")).toBe("a › b › c");
  });
});

describe("read-only worktrees", () => {
  it("is read-only for the reasons the service gives, and for any worktree the open thread does not run in", () => {
    expect(readOnlyOf(worktree({ read_only: true, read_only_reason: "missing" }), "t")).toBe(true);
    expect(readOnlyOf(worktree({ read_only: true, read_only_reason: "locked" }), null)).toBe(true);
    expect(readOnlyOf(worktree({}), "t")).toBe(true);
    expect(readOnlyOf(worktree({ threads: [{ id: "t", title: "", running: false }] }), "t")).toBe(false);
    // With no thread open nothing is being edited, so nothing is claimed.
    expect(readOnlyOf(worktree({}), null)).toBe(false);
  });

  it("names why, so the tab can say it", () => {
    expect(readOnlyWhy(worktree({ read_only: true, read_only_reason: "locked" }), "t")).toBe("locked");
    expect(readOnlyWhy(worktree({}), "t")).toBe("not_this_thread");
    expect(readOnlyWhy(worktree({}), null)).toBeNull();
  });

  it("compares folders by what they name, not how they are written", () => {
    expect(samePath("C:\\code\\app\\", "C:/code/app")).toBe(true);
    expect(samePath("/a/b", "/a/c")).toBe(false);
    // macOS names a temporary folder two ways.
    expect(samePath("/var/folders/x/repo", "/private/var/folders/x/repo")).toBe(true);
    expect(samePath("/privateer/a", "/a")).toBe(false);
  });

  it("finds a folder inside a worktree, and names a file as a thread in that folder does", () => {
    expect(insidePath("/r", "/r")).toBe("");
    expect(insidePath("/r", "/r/packages/web")).toBe("packages/web");
    expect(insidePath("/r", "/rx/web")).toBeNull();
    expect(insidePath("/private/var/r", "/var/r/web")).toBe("web");
    // The thread runs at the worktree's root: paths are the same.
    expect(threadPathOf("/r", "/r", "src/a.ts")).toBe("src/a.ts");
    // It runs in a subfolder: a file below it is named from there, one beside or above it is out of its reach.
    expect(threadPathOf("/r", "/r/packages/web", "packages/web/links.md")).toBe("links.md");
    expect(threadPathOf("/r", "/r/packages/web", "README.md")).toBeNull();
    expect(threadPathOf("/r", "/r/packages/web", "packages/webby/a.md")).toBeNull();
    // Not in this worktree at all.
    expect(threadPathOf("/other", "/r/packages/web", "a.md")).toBeNull();
  });
});
