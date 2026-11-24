import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AWAKE, topics, under } from "./dataRefresh";
import {
  LIMITS_BACKOFF_MS,
  LIMITS_STALE_MS,
  engineLimitsKey,
  limitsAge,
  limitsRefreshOn,
  limitsStale,
  readLimits,
  resetLimitsStore,
} from "./limitsStore";

const MIN = 60_000;

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
  resetLimitsStore();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("limits store", () => {
  it("asks once, then serves a second automatic check inside five minutes from memory", async () => {
    const load = vi.fn().mockResolvedValue({ n: 1 });
    expect(await readLimits("codex", load)).toEqual({ n: 1 });
    vi.advanceTimersByTime(4 * MIN);
    expect(await readLimits("codex", load)).toEqual({ n: 1 });
    expect(load).toHaveBeenCalledTimes(1);
    expect(limitsAge("codex")).toBe(4 * MIN);
  });

  it("goes out again once five minutes have passed", async () => {
    const load = vi.fn().mockResolvedValueOnce({ n: 1 }).mockResolvedValueOnce({ n: 2 });
    await readLimits("codex", load);
    vi.advanceTimersByTime(LIMITS_STALE_MS);
    expect(await readLimits("codex", load)).toEqual({ n: 2 });
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("keeps each engine instance on its own window", async () => {
    const load = vi.fn().mockResolvedValue({});
    await readLimits("codex", load);
    await readLimits("claude", load);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("lets a manual refresh bypass the window and tells the loader to force", async () => {
    const load = vi.fn().mockResolvedValueOnce({ n: 1 }).mockResolvedValueOnce({ n: 2 });
    await readLimits("codex", load);
    expect(await readLimits("codex", load, { force: true })).toEqual({ n: 2 });
    expect(load.mock.calls[0]?.[0]).toBe(false);
    expect(load.mock.calls[1]?.[0]).toBe(true);
    // The forced answer restarts the window.
    vi.advanceTimersByTime(MIN);
    await readLimits("codex", load);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("shares one request between concurrent automatic checks", async () => {
    let resolve!: (value: unknown) => void;
    const load = vi.fn().mockReturnValue(new Promise((done) => (resolve = done)));
    const first = readLimits("codex", load);
    const second = readLimits("codex", load);
    resolve({ n: 1 });
    expect(await first).toEqual({ n: 1 });
    expect(await second).toEqual({ n: 1 });
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("lets the latest request win: a manual refresh replaces a slower automatic one", async () => {
    const slow: { resolve?: (value: unknown) => void } = {};
    const load = vi
      .fn()
      .mockReturnValueOnce(new Promise((done) => (slow.resolve = done)))
      .mockResolvedValueOnce({ n: "manual" });
    const auto = readLimits("codex", load);
    const manual = readLimits("codex", load, { force: true });
    expect(await manual).toEqual({ n: "manual" });
    slow.resolve?.({ n: "auto" });
    await auto;
    // The old answer never overwrites the newer one.
    expect(await readLimits("codex", load)).toEqual({ n: "manual" });
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("does not treat an error as a fresh answer: it retries after the short backoff", async () => {
    const load = vi.fn().mockRejectedValueOnce(new Error("down")).mockResolvedValueOnce({ n: 2 });
    await expect(readLimits("codex", load)).rejects.toThrow("down");
    vi.advanceTimersByTime(LIMITS_BACKOFF_MS - 1);
    await expect(readLimits("codex", load)).rejects.toThrow("down");
    expect(load).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(await readLimits("codex", load)).toEqual({ n: 2 });
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("keeps showing the last answer through a failed check, and retries after the backoff", async () => {
    const load = vi.fn().mockResolvedValueOnce({ n: 1 }).mockRejectedValueOnce(new Error("down")).mockResolvedValueOnce({ n: 3 });
    await readLimits("codex", load);
    vi.advanceTimersByTime(LIMITS_STALE_MS);
    expect(await readLimits("codex", load)).toEqual({ n: 1 });
    vi.advanceTimersByTime(10_000);
    expect(await readLimits("codex", load)).toEqual({ n: 1 });
    expect(load).toHaveBeenCalledTimes(2);
    vi.advanceTimersByTime(LIMITS_BACKOFF_MS);
    expect(await readLimits("codex", load)).toEqual({ n: 3 });
  });

  it("lets a manual refresh retry straight after an error", async () => {
    const load = vi.fn().mockRejectedValueOnce(new Error("down")).mockResolvedValueOnce({ n: 2 });
    await expect(readLimits("codex", load)).rejects.toThrow("down");
    expect(await readLimits("codex", load, { force: true })).toEqual({ n: 2 });
  });

  it("owns no timers, so nothing is left running after a screen unmounts", async () => {
    const load = vi.fn().mockResolvedValue({});
    await readLimits("codex", load);
    await expect(readLimits("claude", vi.fn().mockRejectedValue(new Error("x")))).rejects.toThrow("x");
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("coming back into view", () => {
  const key = engineLimitsKey("codex");

  it("counts an engine read under the engine's own key, whatever screen asked", () => {
    expect(key).toBe("engine:codex");
    expect(limitsStale(key)).toBe(true);
  });

  it("reads again on a wake only once the shared answer is older than the window", async () => {
    const wake = limitsRefreshOn(topics("limits", `limits:codex`), () => key);
    expect(wake(AWAKE)).toBe(true);
    await readLimits(key, vi.fn().mockResolvedValue({}));
    expect(wake(AWAKE)).toBe(false);
    vi.advanceTimersByTime(LIMITS_STALE_MS);
    expect(wake(AWAKE)).toBe(true);
  });

  it("still reads at once for a service limits event, fresh answer or not", async () => {
    const wake = limitsRefreshOn(under("limits"), () => key);
    await readLimits(key, vi.fn().mockResolvedValue({}));
    expect(wake("limits")).toBe(true);
    expect(wake("limits:codex")).toBe(true);
    expect(wake("pulls")).toBe(false);
    expect(wake(AWAKE)).toBe(false);
  });
});
