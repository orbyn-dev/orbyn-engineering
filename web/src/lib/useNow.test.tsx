import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useNow } from "./useNow";

describe("useNow", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("shares one timer between clocks on the same cadence", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const started = vi.spyOn(globalThis, "setInterval");
    const a = renderHook(() => useNow(true));
    const b = renderHook(() => useNow(true));
    expect(started).toHaveBeenCalledTimes(1);
    act(() => vi.advanceTimersByTime(1_000));
    expect(a.result.current).toBe(2_000);
    expect(b.result.current).toBe(2_000);
  });

  it("gives another cadence its own timer", () => {
    vi.useFakeTimers();
    const started = vi.spyOn(globalThis, "setInterval");
    renderHook(() => useNow(true));
    renderHook(() => useNow(true, 30_000));
    expect(started).toHaveBeenCalledTimes(2);
  });

  it("starts no timer while inactive", () => {
    vi.useFakeTimers();
    const started = vi.spyOn(globalThis, "setInterval");
    const { result } = renderHook(() => useNow(false));
    expect(started).not.toHaveBeenCalled();
    expect(result.current).toBe(Date.now());
  });

  it("stops the shared timer when the last clock on that cadence goes away", () => {
    vi.useFakeTimers();
    const stopped = vi.spyOn(globalThis, "clearInterval");
    const a = renderHook(() => useNow(true));
    const b = renderHook(() => useNow(true));
    a.unmount();
    expect(stopped).not.toHaveBeenCalled();
    b.unmount();
    expect(stopped).toHaveBeenCalledTimes(1);
  });
});
