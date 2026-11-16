import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dataChanged } from "../api/shell";
import { COALESCE_MS, registerRefresh, startDataRefresh, topics, under } from "./dataRefresh";
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
