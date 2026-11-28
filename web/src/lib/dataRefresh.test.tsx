import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dataChanged } from "../api/shell";
import { AWAKE, COALESCE_MS, registerRefresh, startDataRefresh, topics, under } from "./dataRefresh";
import { LIMITS_STALE_MS, limitsRefreshOn, readLimits, resetLimitsStore } from "./limitsStore";
import { useAsync, type LoadContext } from "./useAsync";
import { forgetKept } from "./keptData";

let stop: () => void = () => undefined;
beforeEach(() => {
  vi.useFakeTimers();
  stop = startDataRefresh();
});
afterEach(() => {
  stop();
  vi.useRealTimers();
});

describe("topic routing", () => {
  it("reads a screen again only for the topics it shows", async () => {
    const pulls = vi.fn();
    const detail = vi.fn();
    const off = [registerRefresh(topics("pulls"), pulls), registerRefresh(topics("pull:p:3"), detail)];
    dataChanged("pull:p:4");
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(pulls).not.toHaveBeenCalled();
    expect(detail).not.toHaveBeenCalled();
    dataChanged("pull:p:3");
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(detail).toHaveBeenCalledTimes(1);
    expect(pulls).not.toHaveBeenCalled();
    off.forEach((fn) => fn());
  });

  it("matches a family of topics", () => {
    expect(under("models")("models")).toBe(true);
    expect(under("models")("models:claude")).toBe(true);
    expect(under("models")("modelsx")).toBe(false);
    expect(under("limits")("limits:codex")).toBe(true);
  });

  it("stops reading once the screen has gone", async () => {
    const run = vi.fn();
    registerRefresh(topics("forges"), run)();
    dataChanged("forges");
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).not.toHaveBeenCalled();
  });

  it("reads everything again when the stream comes back", async () => {
    const run = vi.fn();
    const off = registerRefresh(topics("forges"), run);
    dataChanged("*");
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).toHaveBeenCalledWith(["*"]);
    off();
  });
});

describe("coalescing", () => {
  it("runs a screen once for a burst, with every topic that named it", async () => {
    const run = vi.fn();
    const off = registerRefresh(under("models"), run);
    dataChanged("models:claude");
    await vi.advanceTimersByTimeAsync(50);
    dataChanged("models:codex");
    dataChanged("models:claude");
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run.mock.calls[0]?.[0]).toEqual(["models:claude", "models:codex"]);
    off();
  });

  it("waits out the window before reading", async () => {
    const run = vi.fn();
    const off = registerRefresh(topics("pulls"), run);
    dataChanged("pulls");
    await vi.advanceTimersByTimeAsync(COALESCE_MS - 1);
    expect(run).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(run).toHaveBeenCalledTimes(1);
    off();
  });
});

describe("a read that shows a topic", () => {
  it("updates what is on screen without a loading state, in the background", async () => {
    forgetKept();
    let rows = ["#1"];
    const contexts: LoadContext[] = [];
    const states: string[] = [];
    const { result } = renderHook(() => {
      const read = useAsync("pulls:open", async (_signal, how) => {
        contexts.push(how);
        return rows;
      }, { keep: true, refreshOn: topics("pulls") });
      states.push(`${read.status}:${read.refreshing}`);
      return read;
    });
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(result.current.data).toEqual(["#1"]);
    states.length = 0;

    rows = ["#1", "#2"];
    dataChanged("pulls");
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    expect(result.current.data).toEqual(["#1", "#2"]);
    expect(states.every((state) => state === "ready:false")).toBe(true);
    expect(contexts.at(-1)).toEqual({ background: true, force: false });
  });

  it("keeps the data on screen when the background read fails", async () => {
    forgetKept();
    let fail = false;
    const { result } = renderHook(() =>
      useAsync("pull:p:3", async () => {
        if (fail) throw new Error("down");
        return "detail";
      }, { refreshOn: topics("pull:p:3") }),
    );
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    fail = true;
    dataChanged("pull:p:3");
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    expect(result.current.status).toBe("ready");
    expect(result.current.data).toBe("detail");
  });

  it("asks for a real read only when the person pressed refresh", async () => {
    forgetKept();
    const contexts: LoadContext[] = [];
    const { result } = renderHook(() => useAsync("pulls:x", async (_signal, how) => { contexts.push(how); return 1; }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    await act(async () => { await result.current.reloadNow(); });
    expect(contexts.at(-1)).toEqual({ background: false, force: true });
    await act(async () => { await result.current.reload(); });
    expect(contexts.at(-1)).toEqual({ background: false, force: false });
  });
});

describe("coming back into view", () => {
  it("reads a screen that asked to notice, once for a return", async () => {
    const run = vi.fn();
    const off = registerRefresh(topics(AWAKE, "limits"), run);
    window.dispatchEvent(new Event("focus"));
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run.mock.calls[0]?.[0]).toEqual([AWAKE]);
    off();
  });

  it("says nothing while the window is still hidden", async () => {
    const run = vi.fn();
    const off = registerRefresh(topics(AWAKE, "limits"), run);
    Object.defineProperty(document, "hidden", { configurable: true, get: () => true });
    try {
      document.dispatchEvent(new Event("visibilitychange"));
      await vi.advanceTimersByTimeAsync(COALESCE_MS);
      expect(run).not.toHaveBeenCalled();
    } finally {
      delete (document as unknown as Record<string, unknown>).hidden;
    }
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).toHaveBeenCalledTimes(1);
    off();
  });

  it("stops hearing the app come back once the app has stopped", async () => {
    const run = vi.fn();
    registerRefresh(topics(AWAKE, "limits"), run);
    stop();
    window.dispatchEvent(new Event("focus"));
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(run).not.toHaveBeenCalled();
  });

  it("does not re-ask a fresh limits answer on a return, and asks once the window has passed", async () => {
    resetLimitsStore();
    const read = vi.fn().mockResolvedValue({ windows: [] });
    const { result } = renderHook(() =>
      useAsync("overview", () => readLimits("overview", read), {
        fresh: true,
        refreshOn: limitsRefreshOn(under("limits"), () => "overview"),
      }),
    );
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(read).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new Event("focus"));
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    expect(read).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(LIMITS_STALE_MS); });
    window.dispatchEvent(new Event("focus"));
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    expect(read).toHaveBeenCalledTimes(2);
    expect(result.current.data).toEqual({ windows: [] });
  });
});

describe("an event that arrives while a read is running", () => {
  it("runs one more read after it, since the running one may already be old", async () => {
    forgetKept();
    const finish: ((value: number) => void)[] = [];
    const contexts: LoadContext[] = [];
    const { result } = renderHook(() =>
      useAsync("pulls:busy", (_signal, how) => new Promise<number>((resolve) => { contexts.push(how); finish.push(resolve); }), { refreshOn: topics("pulls") }),
    );
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(contexts).toHaveLength(1);
    // Two events while the person's first read is still running.
    dataChanged("pulls");
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    dataChanged("pulls");
    await act(async () => { await vi.advanceTimersByTimeAsync(COALESCE_MS); });
    expect(contexts).toHaveLength(1);
    await act(async () => { finish[0]?.(1); await vi.advanceTimersByTimeAsync(0); });
    expect(contexts).toHaveLength(2);
    expect(contexts[1]).toEqual({ background: true, force: false });
    await act(async () => { finish[1]?.(2); await vi.advanceTimersByTimeAsync(0); });
    expect(result.current.data).toBe(2);
  });
});

describe("a read whose joined retry another screen cancelled", () => {
  type Read = { resolve: (value: unknown) => void };
  const screen = (name: string) => {
    const reads: Read[] = [];
    const load = (signal: AbortSignal) =>
      new Promise<unknown>((resolve, reject) => {
        const cancelled = () => reject(new DOMException("The request was aborted.", "AbortError"));
        if (signal.aborted) cancelled();
        else signal.addEventListener("abort", cancelled, { once: true });
        reads.push({ resolve });
      });
    return {
      reads,
      use: () =>
        useAsync(name, (signal, context) => readLimits("shared", () => load(signal), { force: context.force }), {
          fresh: true,
          refreshOn: topics("limits"),
        }),
    };
  };
  const tick = async (ms = 0) => {
    await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
  };

  it("leaves the lane after a foreign cancellation, so a limits event reads again with no second retry", async () => {
    const first = screen("first-screen");
    const second = screen("second-screen");
    const third = screen("third-screen");
    const mountedFirst = renderHook(first.use);
    await tick();
    expect(first.reads).toHaveLength(1);
    const mountedSecond = renderHook(second.use);
    // Both later screens join the one shared check the first screen started.
    const { result } = renderHook(third.use);
    await tick();
    expect(second.reads).toHaveLength(0);
    expect(third.reads).toHaveLength(0);
    // The screen that started the request goes: the second screen's join reads once on its own, and the
    // third screen's join waits on that retry instead of starting a second one.
    mountedFirst.unmount();
    await tick();
    expect(second.reads).toHaveLength(1);
    expect(third.reads).toHaveLength(0);
    // The retry's screen goes: the third screen's one retry is spent, so the retry's abort reaches it with
    // its own signal still live. It must leave the lane rather than wait for an answer nobody will send.
    mountedSecond.unmount();
    await tick();
    expect(third.reads).toHaveLength(0);
    // A limits event now reads the third screen again, which it could not do while the lane was held.
    dataChanged("limits");
    await tick(COALESCE_MS);
    expect(third.reads).toHaveLength(1);
    await act(async () => {
      third.reads[0]?.resolve({ windows: [] });
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toMatchObject({ status: "ready", data: { windows: [] }, refreshing: false });
    // No check was retried twice: the first screen read once, the second once (its one retry), the third
    // only after the event.
    expect(first.reads).toHaveLength(1);
    expect(second.reads).toHaveLength(1);
  });
});
