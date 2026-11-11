import { beforeEach, expect, it } from "vitest";
import type { Thread } from "../../../api/types";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { useDraftStore } from "../../../product/state/draftStore";
import { canStartThreadIn, startThreadIn } from "./startThread";

const worktree = (over: Partial<ExplorerWorktree>): ExplorerWorktree => ({
  id: "w", path: "/code/app", name: "app", branch: "main", detached: false, head: "abc", is_main: true,
  read_only: false, read_only_reason: null, threads: [], dirty: null, ...over,
});
const thread = (path: string) => ({ id: "t", worktree: { path, branch: "x", base: "main" } }) as Thread;

beforeEach(() => {
  useDraftStore.setState({ reuseWorktree: "stale", currentCheckout: false });
  window.location.hash = "#/";
});

it("starts a thread in the primary checkout itself, whatever the device default is", () => {
  startThreadIn("p", worktree({}));
  expect(useDraftStore.getState()).toMatchObject({ reuseWorktree: null, currentCheckout: true });
  expect(window.location.hash).toBe("#/new/p");
});

it("reuses a linked worktree Orbyn made, by its folder's purpose, and does not start in one it did not make", () => {
  const linked = worktree({ is_main: false, path: "/code/app-wt/fix-login", branch: "fix/login" });
  expect(canStartThreadIn(linked, [thread("/code/app-wt/fix-login")])).toBe(true);
  expect(canStartThreadIn(linked, [thread("/elsewhere/other")])).toBe(false);
  expect(canStartThreadIn(linked, [])).toBe(false);
  expect(canStartThreadIn(worktree({}), [])).toBe(true);
  startThreadIn("p", linked);
  expect(useDraftStore.getState()).toMatchObject({ reuseWorktree: "fix-login", currentCheckout: false });
});
