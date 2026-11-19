import { beforeEach, expect, it, vi } from "vitest";
import type { ExplorerTree } from "../../../generated/ExplorerTree";
import type { ExplorerWorktrees } from "../../../generated/ExplorerWorktrees";
import { readTree, readWorktrees } from "./api";
import { levelKey, useExplorer } from "./store";

vi.mock("./api", () => ({ readTree: vi.fn(), readWorktrees: vi.fn() }));

const tree = (path: string): ExplorerTree => ({ worktree_id: "w", path, entries: [], truncated: false,
  total: 0, total_at_least: false, changes: "ready", read_only: false });
const list = (git: boolean): ExplorerWorktrees => ({ project_id: "p", git, worktrees: [] });
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => { useExplorer.getState().reset(); vi.resetAllMocks(); });

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
