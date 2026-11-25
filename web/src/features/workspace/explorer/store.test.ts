import { beforeEach, expect, it, vi } from "vitest";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";
import { readRepoTree, readTree, readWorktrees } from "./api";
import { levelKey, repoLevelKey, useExplorer } from "./store";

vi.mock("./api", () => ({ readRepoTree: vi.fn(), readTree: vi.fn(), readWorktrees: vi.fn() }));

const tree = (path: string): ExplorerTree => ({ worktree_id: "w", path, entries: [], truncated: false,
  total: 0, total_at_least: false, changes: "ready", read_only: false });
const list = (git: boolean): ExplorerWorktrees => ({ project_id: "p", git, worktrees: [] });
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => { useExplorer.getState().reset(); vi.resetAllMocks(); });

it("keeps one repository level per project, thread and repository, and reads them again with the thread and the project", async () => {
  vi.mocked(readRepoTree).mockResolvedValue(tree(""));
  await useExplorer.getState().loadRepoLevel("p", "t", "planner", "");
  await useExplorer.getState().loadRepoLevel("p", "t", undefined, "");
  expect(vi.mocked(readRepoTree).mock.calls).toEqual([["t", "planner", ""], ["t", undefined, ""]]);
  expect(useExplorer.getState().repoLevels[repoLevelKey("p", "t", "planner", "")]?.data?.path).toBe("");
  expect(useExplorer.getState().repoLevels[repoLevelKey("p", "t", undefined, "")]?.data?.path).toBe("");
  await useExplorer.getState().loadRepoLevel("p", "other", "planner", "");
  await useExplorer.getState().loadRepoLevel("q", "t", "planner", "");
  // The thread's repository levels are read again, and only that thread's in that project.
  vi.mocked(readRepoTree).mockClear();
  await useExplorer.getState().reloadRepo("p", "t", { quiet: true });
  expect(vi.mocked(readRepoTree).mock.calls).toEqual([["t", "planner", ""], ["t", undefined, ""]]);
  // A project's reload reads its repository levels as it reads its worktree levels.
  await useExplorer.getState().loadWorktrees("p", { thread: null });
  await useExplorer.getState().loadLevel("p", "w", "");
  vi.mocked(readRepoTree).mockClear();
  await useExplorer.getState().reload("p", { thread: null, quiet: true });
  expect(vi.mocked(readRepoTree).mock.calls).toEqual([["t", "planner", ""], ["t", undefined, ""], ["other", "planner", ""]]);
  // Reset forgets them with everything else.
  useExplorer.getState().reset();
  expect(useExplorer.getState().repoLevels).toEqual({});
});

it("one change during an old folder read queues a fresh read and new data wins", async () => {
  const old = deferred<ExplorerTree>();
  vi.mocked(readTree).mockReturnValueOnce(old.promise).mockResolvedValueOnce(tree("new"));
  const first = useExplorer.getState().loadLevel("p", "w", "");
  const event = useExplorer.getState().reload("p", { thread: null, quiet: true });
  expect(readTree).toHaveBeenCalledTimes(1);
  old.resolve(tree("old"));
  await Promise.all([first, event]);
  expect(readTree).toHaveBeenCalledTimes(2);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]?.data?.path).toBe("new");
});

it("a burst during a worktree read queues one rerun", async () => {
  const old = deferred<ExplorerWorktrees>();
  vi.mocked(readWorktrees).mockReturnValueOnce(old.promise).mockResolvedValueOnce(list(false));
  const first = useExplorer.getState().loadWorktrees("p", { thread: null });
  const events = Array.from({ length: 50 }, () => useExplorer.getState().reload("p", { thread: null, quiet: true }));
  old.resolve(list(true));
  await Promise.all([first, ...events]);
  expect(readWorktrees).toHaveBeenCalledTimes(2);
  expect(useExplorer.getState().worktrees.p?.data?.git).toBe(false);
});

it("a change in the completion microtask gap starts a fresh request", async () => {
  const old = deferred<ExplorerTree>();
  vi.mocked(readTree).mockReturnValueOnce(old.promise).mockResolvedValueOnce(tree("fresh"));
  const first = useExplorer.getState().loadLevel("p", "w", "");
  old.resolve(tree("old"));
  const event = new Promise<void>((resolve, reject) => {
    queueMicrotask(() => { useExplorer.getState().reload("p", { thread: null, quiet: true }).then(resolve, reject); });
  });
  await Promise.all([first, event]);
  expect(readTree).toHaveBeenCalledTimes(2);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]?.data?.path).toBe("fresh");
});

it("an event after a forced refresh does not reuse an older normal flight", async () => {
  const normal = deferred<ExplorerTree>();
  vi.mocked(readTree).mockReturnValueOnce(normal.promise)
    .mockResolvedValueOnce(tree("forced-before-edit")).mockResolvedValueOnce(tree("after-edit"));
  const first = useExplorer.getState().loadLevel("p", "w", "");
  await useExplorer.getState().loadLevel("p", "w", "", { refresh: true });
  const event = useExplorer.getState().reload("p", { thread: null, quiet: true });
  normal.resolve(tree("old-normal"));
  await Promise.all([first, event]);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]?.data?.path).toBe("after-edit");
  expect(readTree).toHaveBeenCalledTimes(3);
});

it("a later request owns the slot and prevents an older queued rerun", async () => {
  const old = deferred<ExplorerWorktrees>();
  vi.mocked(readWorktrees).mockReturnValueOnce(old.promise).mockResolvedValueOnce(list(false));
  const first = useExplorer.getState().loadWorktrees("p", { thread: "old" });
  const event = useExplorer.getState().loadWorktrees("p", { thread: "old", quiet: true });
  await useExplorer.getState().loadWorktrees("p", { thread: "new", quiet: true });
  old.resolve(list(true));
  await Promise.all([first, event]);
  expect(readWorktrees).toHaveBeenCalledTimes(2);
  expect(useExplorer.getState().worktrees.p?.data?.git).toBe(false);
});

it("reset drops a queued rerun and fences an old response from a new same-key request", async () => {
  const old = deferred<ExplorerTree>();
  const fresh = deferred<ExplorerTree>();
  vi.mocked(readTree).mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise).mockResolvedValue(tree("unexpected"));
  const first = useExplorer.getState().loadLevel("p", "w", "");
  const event = useExplorer.getState().loadLevel("p", "w", "", { quiet: true });
  useExplorer.getState().reset();
  const afterReset = useExplorer.getState().loadLevel("p", "w", "");
  old.resolve(tree("old"));
  await Promise.all([first, event]);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]?.data).toBeNull();
  const sameQuestion = useExplorer.getState().loadLevel("p", "w", "");
  expect(readTree).toHaveBeenCalledTimes(2);
  fresh.resolve(tree("fresh"));
  await Promise.all([afterReset, sameQuestion]);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]?.data?.path).toBe("fresh");
});

it("a failed event rerun preserves the data returned by the initial read", async () => {
  const old = deferred<ExplorerTree>();
  vi.mocked(readTree).mockReturnValueOnce(old.promise).mockRejectedValueOnce(new Error("offline"));
  const first = useExplorer.getState().loadLevel("p", "w", "");
  const event = useExplorer.getState().loadLevel("p", "w", "", { quiet: true });
  old.resolve(tree("kept"));
  await Promise.all([first, event]);
  expect(useExplorer.getState().levels[levelKey("p", "w", "")]).toMatchObject({ data: { path: "kept" }, loading: false, error: null });
});
