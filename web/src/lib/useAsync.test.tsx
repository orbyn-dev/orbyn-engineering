import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readKept, writeKept } from "./keptData";
import { useAsync } from "./useAsync";

describe("useAsync", () => {
  it("keeps confirmed data when a read started before setData finishes late", async () => {
    let finishRead!: (value: string) => void;
    const { result } = renderHook(() => useAsync("task", () => new Promise<string>((resolve) => {
      finishRead = resolve;
    })));
    await waitFor(() => expect(finishRead).toBeDefined());
    act(() => result.current.setData("saved task"));
    await act(async () => { finishRead("old task"); });
    expect(result.current.data).toBe("saved task");
    expect(result.current.status).toBe("ready");
  });

  it("applies only the current key's result", async () => {
    const resolvers: Record<string, (value: string) => void> = {};
    const { result, rerender } = renderHook(
      ({ id }) =>
        useAsync(id, () => new Promise<string>((resolve) => (resolvers[id] = resolve))),
      { initialProps: { id: "a" } },
    );
    await waitFor(() => expect(resolvers.a).toBeDefined());
    rerender({ id: "b" });
    await waitFor(() => expect(resolvers.b).toBeDefined());
    act(() => resolvers.b?.("b result"));
    await waitFor(() => expect(result.current.data).toBe("b result"));
    act(() => resolvers.a?.("a result"));
    await new Promise((resolve) => setTimeout(resolve));
    expect(result.current.data).toBe("b result");
  });

  it("keeps prior data on a failed reload and recovers on retry", async () => {
    let fail = false;
    const { result } = renderHook(() =>
      useAsync("k", async () => {
        if (fail) throw new Error("no");
        return "ok";
      }),
    );
    await waitFor(() => expect(result.current.status).toBe("ready"));
    fail = true;
    await act(async () => {
      await result.current.reload();
    });
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.data).toBe("ok");
    fail = false;
    await act(async () => {
      await result.current.reload();
    });
    await waitFor(() => expect(result.current.status).toBe("ready"));
  });

  describe("revisiting a key", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    const slow = (value: string, ms = 1000) => () =>
      new Promise<string>((resolve) => setTimeout(() => resolve(value), ms));

    it("draws the kept data at once, then replaces it when the read finishes", async () => {
      const first = renderHook(() => useAsync("screen", slow("first"), { keep: true }));
      expect(first.result.current.status).toBe("loading");
      await act(() => vi.advanceTimersByTimeAsync(1000));
      expect(first.result.current.data).toBe("first");
      first.unmount();

      const revisit = renderHook(() => useAsync("screen", slow("second"), { keep: true }));
      expect(revisit.result.current).toMatchObject({ status: "ready", data: "first", refreshing: true });
      await act(() => vi.advanceTimersByTimeAsync(999));
      expect(revisit.result.current).toMatchObject({ data: "first", refreshing: true });
      await act(() => vi.advanceTimersByTimeAsync(1));
      expect(revisit.result.current).toMatchObject({ status: "ready", data: "second", refreshing: false });
    });

    it("starts a key it never read as loading, and shows kept data when switching back", async () => {
      const { result, rerender } = renderHook(({ id }) => useAsync(id, slow(id), { keep: true }), { initialProps: { id: "a" } });
      await act(() => vi.advanceTimersByTimeAsync(1000));
      rerender({ id: "b" });
      expect(result.current.status).toBe("loading");
      await act(() => vi.advanceTimersByTimeAsync(1000));
      rerender({ id: "a" });
      expect(result.current).toMatchObject({ status: "ready", data: "a", refreshing: true });
    });

    it("keeps nothing unless the read opts in", async () => {
      const first = renderHook(() => useAsync("file:a.txt", slow("contents")));
      await act(() => vi.advanceTimersByTimeAsync(1000));
      expect(first.result.current.data).toBe("contents");
      first.unmount();
      const again = renderHook(() => useAsync("file:a.txt", slow("contents")));
      expect(again.result.current.status).toBe("loading");
    });

    it("does not put a late mutation for one key under the key the screen moved to", async () => {
      const { result, rerender } = renderHook(({ id }) => useAsync(id, slow(id), { keep: true }), { initialProps: { id: "a" } });
      await act(() => vi.advanceTimersByTimeAsync(1000));
      const setForA = result.current.setData;
      rerender({ id: "b" });
      await act(() => vi.advanceTimersByTimeAsync(1000));
      expect(result.current.data).toBe("b");
      act(() => setForA("changed a"));
      // The screen still shows b, and b's kept answer is untouched.
      expect(result.current.data).toBe("b");
      expect(readKept("b")).toBe("b");
      expect(readKept("a")).toBe("changed a");
    });

    it("sweeps expired entries on a write and refuses an answer too large to keep", async () => {
      writeKept("stale", "x");
      vi.advanceTimersByTime(10 * 60_000 + 1);
      writeKept("fresh", "y");
      expect(readKept("stale")).toBeUndefined();
      writeKept("huge", "z".repeat(1_500_000));
      expect(readKept("huge")).toBeUndefined();
      writeKept("bytes", new Uint8Array(2_000_000));
      expect(readKept("bytes")).toBeUndefined();
      expect(readKept("fresh")).toBe("y");
    });

    it("lets data go after ten minutes", async () => {
      const { result } = renderHook(() => useAsync("old", slow("x"), { keep: true }));
      await act(() => vi.advanceTimersByTimeAsync(1000));
      expect(result.current.data).toBe("x");
      vi.advanceTimersByTime(10 * 60_000 + 1);
      expect(readKept("old")).toBeUndefined();
    });

    it("keeps a failed read from replacing the kept data with nothing", async () => {
      const { result, unmount } = renderHook(() => useAsync("flaky", slow("good"), { keep: true }));
      await act(() => vi.advanceTimersByTimeAsync(1000));
      unmount();
      const again = renderHook(() => useAsync("flaky", () => Promise.reject(new Error("down")), { keep: true }));
      await act(() => vi.advanceTimersByTimeAsync(0));
      expect(again.result.current.status).toBe("error");
      expect(again.result.current.data).toBe("good");
      expect(result.current.data).toBe("good");
    });
  });
});
