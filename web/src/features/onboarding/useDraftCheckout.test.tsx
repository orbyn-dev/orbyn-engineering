import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { holdFetch, wire } from "../../wire.test-support";
import { useDraftCheckout } from "./useDraftCheckout";

const a = { id: "a", is_git: true }, b = { id: "b", is_git: true };
let mock: MockApi;
/** The device setting that says where new threads start: "local" or "worktree". */
const setDefault = (value: "local" | "worktree") => mock.handle("PUT", "/api/preferences", { values: { default_thread_env_mode: value } });
const reads = () => wire().filter((call) => call.path === "/api/preferences");
beforeEach(() => { mock = installMockFetch(); });

it("waits for the effective file-backed default and lets an explicit pick win", async () => {
  setDefault("worktree");
  const slow = holdFetch("GET", "/api/preferences");
  const { result } = renderHook(() => useDraftCheckout(a, null));
  expect(result.current.loading).toBe(true);
  act(() => result.current.setWorktree(null));
  await act(async () => slow.release());
  expect(result.current.worktree).toBeNull(); expect(result.current.loading).toBe(false);
});

it("starts in the project's own checkout when asked, whatever the device default is", async () => {
  setDefault("worktree");
  const { result } = renderHook(() => useDraftCheckout(a, null, true));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.worktree).toBeNull();
});

it("uses a worktree default without requiring a manual checkout pick", async () => {
  setDefault("worktree");
  const { result } = renderHook(() => useDraftCheckout(a, null));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.worktree).toBe("");
});

it("fences a previous project's late result and resets explicit scope", async () => {
  setDefault("worktree");
  const slow = holdFetch("GET", "/api/preferences");
  const { result, rerender } = renderHook(({ project }) => useDraftCheckout(project, null), { initialProps: { project: a } });
  act(() => result.current.setWorktree("picked-a"));
  // The service now answers "local", and project a's reply, sent while it said "worktree", is still on its way.
  setDefault("local");
  rerender({ project: b });
  expect(result.current.loading).toBe(true);
  await act(async () => slow.release());
  expect(reads()).toHaveLength(2);
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.worktree).toBeNull();
});

it("keeps reused worktrees and never applies a worktree to scratch or a plain folder", async () => {
  setDefault("local");
  const { result, rerender } = renderHook(({ project, reused }) => useDraftCheckout(project, reused), {
    initialProps: { project: a as typeof a | null, reused: "existing" as string | null },
  });
  expect(result.current.worktree).toBe("existing");
  await waitFor(() => expect(reads()).toHaveLength(1));
  rerender({ project: { id: "plain", is_git: false }, reused: null });
  expect(result.current.worktree).toBeNull(); expect(result.current.loading).toBe(false);
  rerender({ project: null, reused: null }); expect(result.current.worktree).toBeNull();
  expect(reads()).toHaveLength(1);
});

it("reports an unavailable default instead of sending with a guessed checkout", async () => {
  mock.fail("/api/preferences", 1, 503, "unavailable", "Settings unavailable");
  const { result } = renderHook(() => useDraftCheckout(a, null));
  await waitFor(() => expect(result.current.error).toBe("Settings unavailable"));
  act(() => result.current.setWorktree(null));
  expect(result.current.error).toBeNull(); expect(result.current.worktree).toBeNull();
});
