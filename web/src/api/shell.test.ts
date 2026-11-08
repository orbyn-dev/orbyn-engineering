import { afterEach, describe, expect, it, vi } from "vitest";
import { followShell, zoomKeyAction } from "./shell";

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("zoomKeyAction", () => {
  it("catches the keypad and the shifted plus", () => {
    expect(zoomKeyAction(key({ metaKey: true, code: "NumpadAdd", key: "+" }))).toBe("zoom_in");
    expect(zoomKeyAction(key({ ctrlKey: true, code: "NumpadSubtract", key: "-" }))).toBe("zoom_out");
    expect(zoomKeyAction(key({ ctrlKey: true, code: "Numpad0", key: "0" }))).toBe("zoom_reset");
    expect(zoomKeyAction(key({ metaKey: true, shiftKey: true, code: "Equal", key: "+" }))).toBe("zoom_in");
  });

  it("leaves ordinary keys alone", () => {
    expect(zoomKeyAction(key({ code: "NumpadAdd", key: "+" }))).toBeNull();
    expect(zoomKeyAction(key({ metaKey: true, code: "Digit0", key: "0" }))).toBeNull();
    expect(zoomKeyAction(key({ metaKey: true, altKey: true, code: "NumpadAdd", key: "+" }))).toBeNull();
  });
});

describe("followShell", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("retries a dropped stream within 250 ms, then backs off, so a menu command is not lost for seconds", async () => {
    vi.useFakeTimers();
    const fetches = vi.fn(async () => {
      throw new Error("down");
    });
    vi.stubGlobal("fetch", fetches);
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    expect(fetches).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(250);
    expect(fetches).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(500);
    expect(fetches).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetches).toHaveBeenCalledTimes(4);
    stop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(fetches).toHaveBeenCalledTimes(4);
  });

  it("starts over at 250 ms once a stream has connected", async () => {
    vi.useFakeTimers();
    let calls = 0;
    vi.stubGlobal("fetch", vi.fn(async () => {
      calls += 1;
      if (calls < 3) throw new Error("down");
      return new Response(new ReadableStream({ start: (controller) => controller.close() }), { status: 200 });
    }));
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(250 + 500);
    expect(calls).toBe(3);
    // The third call connected and ended; the next try comes in 250 ms, not 1 s.
    await vi.advanceTimersByTimeAsync(260);
    expect(calls).toBe(4);
    stop();
  });
});
