import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readKept, writeKept } from "./keptData";
import { type LoadContext, useAsync } from "./useAsync";

describe("useAsync", () => {
  it("keeps fresh data cleared while a queued event follows a foreground read", async () => {
    const reads: ((value: string) => void)[] = [];
    const { result } = renderHook(() => useAsync("fresh-queued", () => new Promise<string>((resolve) => reads.push(resolve)), { fresh: true }));
    await waitFor(() => expect(reads).toHaveLength(1));
    await act(async () => { reads[0]?.("first account"); });
    let foreground!: Promise<void>;
    act(() => { foreground = result.current.reloadNow(); });
    expect(result.current.data).toBeNull();
    await act(async () => { await result.current.refreshQuietly(); });
    await act(async () => { reads[1]?.("superseded snapshot"); await foreground; });
    expect(reads).toHaveLength(3);
    expect(result.current.status).toBe("loading");
    expect(result.current.data).toBeNull();
    await act(async () => { reads[2]?.("current account"); });
    expect(result.current.data).toBe("current account");
  });

  it("fresh reads discard unvalidated data even for a failed background reload", async () => {
    let fail = false;
    const { result } = renderHook(() => useAsync("private-limits", async () => {
      if (fail) throw new Error("account unknown");
      return "prior account";
    }, { fresh: true, keep: true }));
    await waitFor(() => expect(result.current.data).toBe("prior account"));
    fail = true;
    await act(async () => { await result.current.refreshQuietly(); });
    expect(result.current.status).toBe("error");
    expect(result.current.data).toBeNull();
    expect(readKept("private-limits")).toBeUndefined();
  });

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

  describe("refreshQuietly", () => {
    type Reads = { signal: AbortSignal; resolve: (value: string) => void; reject: (error: unknown) => void }[];
    const deferred = () => {
      const reads: Reads = [];
      const load = (signal: AbortSignal) =>
        new Promise<string>((resolve, reject) => {
          reads.push({ signal, resolve, reject });
        });
      return { reads, load };
    };
    const shown = async (reads: Reads, result: { current: { data: string | null } }) => {
      await waitFor(() => expect(reads.length).toBeGreaterThan(0));
      await act(async () => reads[0]?.resolve("one"));
      expect(result.current.data).toBe("one");
    };

    it("replaces the data without a loading state or a refreshing flash", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      let pending!: Promise<void>;
      act(() => { pending = result.current.refreshQuietly(); });
      expect(result.current).toMatchObject({ status: "ready", data: "one", refreshing: false });
      await act(async () => reads[1]?.resolve("two"));
      await pending;
      expect(result.current).toMatchObject({ status: "ready", data: "two", refreshing: false });
    });

    it("never puts an error over the data on screen", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.refreshQuietly(); });
      await act(async () => reads[1]?.reject(new Error("down")));
      expect(result.current).toMatchObject({ status: "ready", data: "one", error: null, refreshing: false });
    });

    it("waits behind a person's read and reads once more after it", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => { void result.current.refreshQuietly(); });
      expect(reads).toHaveLength(2);
      expect(reads[1]?.signal.aborted).toBe(false);
      await act(async () => reads[1]?.resolve("mine"));
      expect(result.current.data).toBe("mine");
      expect(reads).toHaveLength(3);
      await act(async () => reads[2]?.resolve("newer"));
      expect(result.current.data).toBe("newer");
    });

    it("is cut short by a person's read, and its late answer is ignored", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.refreshQuietly(); });
      act(() => { void result.current.reload(); });
      expect(reads[1]?.signal.aborted).toBe(true);
      await act(async () => reads[2]?.resolve("mine"));
      await act(async () => reads[1]?.resolve("late"));
      expect(result.current.data).toBe("mine");
    });

    it("does not abort the follow-up that runs after a person's read", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => { void result.current.refreshQuietly(); });
      await act(async () => reads[1]?.resolve("mine"));
      expect(reads).toHaveLength(3);
      act(() => { void result.current.refreshQuietly(); });
      expect(reads[2]?.signal.aborted).toBe(false);
      expect(reads).toHaveLength(3);
      await act(async () => reads[2]?.resolve("follow-up"));
      expect(result.current.data).toBe("follow-up");
    });

    it("releases the lane when setData lands over a read in flight, so a later quiet read still runs", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => result.current.setData("saved"));
      await act(async () => reads[1]?.resolve("late"));
      expect(result.current.data).toBe("saved");
      act(() => { void result.current.refreshQuietly(); });
      expect(reads).toHaveLength(3);
      await act(async () => reads[2]?.resolve("fresh"));
      expect(result.current.data).toBe("fresh");
    });

    it("runs the one read parked behind a person's read when setData lands", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => { void result.current.refreshQuietly(); });
      act(() => { void result.current.refreshQuietly(); });
      expect(reads).toHaveLength(2);
      act(() => result.current.setData("saved"));
      expect(reads).toHaveLength(3);
      await act(async () => reads[2]?.resolve("fresh"));
      expect(result.current.data).toBe("fresh");
    });

    it("parks a tick that lands during the follow-up and reads once more after it", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => { void result.current.refreshQuietly(); });
      await act(async () => reads[1]?.resolve("mine"));
      expect(reads).toHaveLength(3);
      // The follow-up holds the lane: a tick during it waits instead of cutting it short.
      act(() => { void result.current.refreshQuietly(); });
      expect(reads).toHaveLength(3);
      expect(reads[2]?.signal.aborted).toBe(false);
      await act(async () => reads[2]?.resolve("follow-up"));
      expect(result.current.data).toBe("follow-up");
      expect(reads).toHaveLength(4);
      await act(async () => reads[3]?.resolve("after the tick"));
      expect(result.current.data).toBe("after the tick");
    });

    it("drops a quiet read's answer when setData lands over it, without starting another", async () => {
      const { reads, load } = deferred();
      const { result } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.refreshQuietly(); });
      act(() => result.current.setData("saved"));
      await act(async () => reads[1]?.resolve("late"));
      expect(result.current.data).toBe("saved");
      expect(reads).toHaveLength(2);
    });

    it("forgets a parked read when the screen goes, so setData after that starts none", async () => {
      const { reads, load } = deferred();
      const { result, unmount } = renderHook(() => useAsync("q", load));
      await shown(reads, result);
      act(() => { void result.current.reload(); });
      act(() => { void result.current.refreshQuietly(); });
      const setData = result.current.setData;
      expect(reads).toHaveLength(2);
      unmount();
      // The parked read died with the screen: the late save must not wake it.
      act(() => setData("late save"));
      expect(reads).toHaveLength(2);
    });
  });

  describe("keepOnManual", () => {
    type Read = { context: LoadContext; resolve: (value: string) => void; reject: (error: unknown) => void };
    const gated = () => {
      const reads: Read[] = [];
      const load = (_signal: AbortSignal, context: LoadContext) =>
        new Promise<string>((resolve, reject) => reads.push({ context, resolve, reject }));
      return { reads, load };
    };
    const options = { fresh: true, keepOnManual: true };
    const shown = async (reads: Read[], result: { current: { data: string | null } }) => {
      await waitFor(() => expect(reads).toHaveLength(1));
      await act(async () => reads[0]?.resolve("prior account"));
      expect(result.current.data).toBe("prior account");
    };

    it("keeps the figures through a person's refresh, while it runs and after it fails", async () => {
      const { reads, load } = gated();
      const { result } = renderHook(() => useAsync("limits", load, options));
      await shown(reads, result);
      let pending!: Promise<void>;
      act(() => { pending = result.current.reloadNow(); });
      await waitFor(() => expect(reads).toHaveLength(2));
      expect(reads[1]?.context).toEqual({ background: false, force: true });
      expect(result.current).toMatchObject({ status: "ready", data: "prior account", refreshing: true });
      await act(async () => { reads[1]?.reject(new Error("down")); await pending; });
      expect(result.current).toMatchObject({ status: "error", data: "prior account", refreshing: false });
    });

    it("still blanks on an event read, because the figures may be another account's", async () => {
      const { reads, load } = gated();
      const { result } = renderHook(() => useAsync("limits", load, options));
      await shown(reads, result);
      let pending!: Promise<void>;
      act(() => { pending = result.current.refreshQuietly(); });
      await waitFor(() => expect(reads).toHaveLength(2));
      // An event is not a person: it asks without force and shows nothing while it runs.
      expect(reads[1]?.context).toEqual({ background: true, force: false });
      expect(result.current).toMatchObject({ status: "loading", data: null, refreshing: true });
      await act(async () => { reads[1]?.reject(new Error("down")); await pending; });
      expect(result.current).toMatchObject({ status: "error", data: null, refreshing: false });
    });

    it("blanks for a reload the screen starts itself, without a refresh control", async () => {
      const { reads, load } = gated();
      const { result } = renderHook(() => useAsync("limits", load, options));
      await shown(reads, result);
      let pending!: Promise<void>;
      act(() => { pending = result.current.reload(); });
      await waitFor(() => expect(reads).toHaveLength(2));
      expect(reads[1]?.context).toEqual({ background: false, force: false });
      expect(result.current).toMatchObject({ status: "loading", data: null });
      await act(async () => { reads[1]?.reject(new Error("down")); await pending; });
      expect(result.current).toMatchObject({ status: "error", data: null });
    });

    it("shows the current account's figures after a failed event read, with no stale mix", async () => {
      const { reads, load } = gated();
      const { result } = renderHook(() => useAsync("limits", load, options));
      await shown(reads, result);
      let pending!: Promise<void>;
      act(() => { pending = result.current.refreshQuietly(); });
      await waitFor(() => expect(reads).toHaveLength(2));
      await act(async () => { reads[1]?.reject(new Error("down")); await pending; });
      expect(result.current.data).toBeNull();
      act(() => { void result.current.reloadNow(); });
      await waitFor(() => expect(reads).toHaveLength(3));
      await act(async () => reads[2]?.resolve("current account"));
      expect(result.current).toMatchObject({ status: "ready", data: "current account", error: null });
    });

    it("drops the figures for another key, however the read was made", async () => {
      const { reads, load } = gated();
      const { result, rerender } = renderHook(({ id }) => useAsync(id, load, options), { initialProps: { id: "engine:codex" } });
      await shown(reads, result);
      rerender({ id: "engine:claude" });
      expect(result.current).toMatchObject({ status: "loading", data: null });
    });
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
