import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import type { ResolvedThreadPath } from "../../generated/ResolvedThreadPath";
import { useUi } from "../state/ui";
import { useWorkspace } from "../state/workspace";
import { useResolvedThreadPath } from "./useResolvedThreadPath";

const thread: Thread = { id: "t", project_id: "p", title: "Scope", engine: "codex", model: null,
  cwd: "/repo/packages/web", worktree: null, skill_profile: null, scratch: false, active_run: null,
  last_run_state: null, created_at: "", updated_at: "" };
const reference = (path: string, nonce: number) => ({ threadId: thread.id, cwd: thread.cwd, path, basis: "thread" as const, nonce });
const response = (path: string): ResolvedThreadPath => ({ cwd: thread.cwd, checkout_path: path, repository_path: `packages/web/${path}` });
const prepare = () => {
  window.history.replaceState(null, "", "#/threads/t");
  useWorkspace.setState({ threads: { t: thread } });
};
afterEach(() => vi.restoreAllMocks());

it("a competing click wins even when the earlier metadata read finishes last", async () => {
  prepare();
  const pending: ((value: ResolvedThreadPath) => void)[] = [];
  vi.spyOn(api, "resolveThreadPath").mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
  const view = renderHook(({ path, nonce }) => useResolvedThreadPath("t", reference(path, nonce)), { initialProps: { path: "one.txt", nonce: 1 } });
  await waitFor(() => expect(pending).toHaveLength(1));
  view.rerender({ path: "two.txt", nonce: 2 });
  await waitFor(() => expect(pending).toHaveLength(2));
  act(() => pending[1]?.(response("two.txt")));
  await waitFor(() => expect(view.result.current.value?.checkout_path).toBe("two.txt"));
  act(() => pending[0]?.(response("one.txt")));
  await waitFor(() => expect(view.result.current.value?.checkout_path).toBe("two.txt"));
});

it("route and cwd changes invalidate an in-flight navigation and its origin", async () => {
  prepare();
  let finish: (value: ResolvedThreadPath) => void = () => undefined;
  const resolve = vi.spyOn(api, "resolveThreadPath").mockImplementation(() => new Promise((done) => { finish = done; }));
  const view = renderHook(() => useResolvedThreadPath("t", reference("one.txt", 1)));
  await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
  act(() => {
    window.history.replaceState(null, "", "#/settings/projects");
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    window.history.replaceState(null, "", "#/threads/t");
    useWorkspace.setState({ threads: { t: { ...thread, cwd: "/other" } } });
    finish(response("one.txt"));
  });
  await waitFor(() => expect(view.result.current.value).toBeNull());
  expect(resolve).toHaveBeenCalledTimes(1);
});

it("a later thread cannot consume an earlier thread's reference", async () => {
  prepare();
  useWorkspace.setState({ threads: { t: thread, other: { ...thread, id: "other" } } });
  const resolve = vi.spyOn(api, "resolveThreadPath");
  const view = renderHook(() => useResolvedThreadPath("other", reference("one.txt", 1)));
  await waitFor(() => expect(view.result.current.value).toBeNull());
  expect(resolve).not.toHaveBeenCalled();
});

it("consuming an older file or diff intent cannot clear a newer click", () => {
  const origin = { threadId: thread.id, cwd: thread.cwd };
  useUi.getState().revealFile("one.txt", null, origin);
  const first = useUi.getState().fileReveal!;
  useUi.getState().revealFile("two.txt", null, origin);
  useUi.getState().consumeFileReveal(first.nonce);
  expect(useUi.getState().fileReveal?.path).toBe("two.txt");
  useUi.getState().revealDiff("packages/web/one.txt", origin);
  const diff = useUi.getState().diffReveal!;
  useUi.getState().revealDiff("README.md", origin);
  useUi.getState().consumeDiffReveal(diff.nonce);
  expect(useUi.getState().diffReveal?.path).toBe("README.md");
  useUi.getState().consumeFileReveal(first.nonce);
  useUi.getState().consumeDiffReveal(useUi.getState().diffReveal!.nonce);
  expect(useUi.getState().fileReveal).toBeNull();
  expect(useUi.getState().diffReveal).toBeNull();
});

it("route cancellation clears transient navigation even with no mounted surface", () => {
  const origin = { threadId: thread.id, cwd: thread.cwd };
  useUi.getState().revealFile("one.txt", null, origin);
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  expect(useUi.getState().fileReveal).toBeNull();
  useUi.getState().revealDiff("README.md", origin);
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  expect(useUi.getState().diffReveal).toBeNull();
});

it("names the reference's repository as repo_id, and names none for the primary one", async () => {
  prepare();
  const resolve = vi.spyOn(api, "resolveThreadPath").mockResolvedValue(response("one.txt"));
  // The primary repository is the service's default, so the request carries no repo_id at all.
  renderHook(() => useResolvedThreadPath("t", reference("one.txt", 1)));
  await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
  expect(resolve.mock.calls[0]?.[1]).toEqual({ path: "one.txt", basis: "thread", cwd: thread.cwd });
  // A file of another repository of a multi-repository thread is resolved in that repository.
  renderHook(() => useResolvedThreadPath("t", { ...reference("one.txt", 1), repoId: "planner" }));
  await waitFor(() => expect(resolve).toHaveBeenCalledTimes(2));
  expect(resolve.mock.calls[1]?.[1]).toEqual({ path: "one.txt", basis: "thread", cwd: thread.cwd, repo_id: "planner" });
});

it("cwd cancellation survives an absent surface and a move back to the original folder", () => {
  prepare();
  useUi.getState().revealFile("one.txt", null, { threadId: thread.id, cwd: thread.cwd });
  useWorkspace.setState({ threads: { t: { ...thread, cwd: "/other" } } });
  useWorkspace.setState({ threads: { t: thread } });
  expect(useUi.getState().fileReveal).toBeNull();
});
