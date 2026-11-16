import { afterEach, describe, expect, it, vi } from "vitest";
import { ALL_DATA, askPresence, canAskPresence, followShell, onDataChanged, PresenceFailure, readShellStatus, zoomKeyAction } from "./shell";

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

  it("starts over at 250 ms after a stream that delivered something", async () => {
    vi.useFakeTimers();
    let calls = 0;
    vi.stubGlobal("fetch", vi.fn(async () => {
      calls += 1;
      if (calls < 3) throw new Error("down");
      const body = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('data: {"type":"changed"}\n\n'));
          controller.close();
        },
      });
      return new Response(body, { status: 200 });
    }));
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(250 + 500);
    expect(calls).toBe(3);
    // The third call delivered an event and ended; the next try comes in 250 ms, not 1 s.
    await vi.advanceTimersByTimeAsync(260);
    expect(calls).toBe(4);
    stop();
  });

  it("backs off from a server that accepts the stream and closes it at once", async () => {
    vi.useFakeTimers();
    const fetches = vi.fn(async () => new Response(new ReadableStream({ start: (controller) => controller.close() }), { status: 200 }));
    vi.stubGlobal("fetch", fetches);
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    expect(fetches).toHaveBeenCalledTimes(1);
    // 250, 500, 1000, 2000 ms between tries: not a retry every 250 ms for ever.
    await vi.advanceTimersByTimeAsync(250);
    await vi.advanceTimersByTimeAsync(500);
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetches).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(1500);
    expect(fetches).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(600);
    expect(fetches).toHaveBeenCalledTimes(5);
    stop();
  });

  it("treats a silent stream that stayed up for a while and then errored as healthy", async () => {
    vi.useFakeTimers();
    let calls = 0;
    vi.stubGlobal("fetch", vi.fn(async () => {
      calls += 1;
      if (calls === 1) {
        // Comes up, says nothing for 6 s, then the connection breaks.
        const body = new ReadableStream({
          start(controller) {
            setTimeout(() => controller.error(new Error("reset")), 6000);
          },
        });
        return new Response(body, { status: 200 });
      }
      throw new Error("down");
    }));
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(6000);
    // It was healthy, so the next try is the first retry: 250 ms, not a backed-off wait.
    await vi.advanceTimersByTimeAsync(260);
    expect(calls).toBe(2);
    stop();
  });

  it("says everything may have changed after a real gap, not on the first connection or a quick retry", async () => {
    vi.useFakeTimers();
    let calls = 0;
    const open = () => new Response(new ReadableStream({ start: (controller) => { setTimeout(() => controller.close(), 10); } }), { status: 200 });
    // Calls 2 and 3 are quick retries after a closed stream; 4 to 7 fail for over ten seconds.
    vi.stubGlobal("fetch", vi.fn(async () => {
      calls += 1;
      if (calls >= 4 && calls <= 7) throw new Error("down");
      return open();
    }));
    const heard: string[] = [];
    const off = onDataChanged((what) => heard.push(what));
    const stop = followShell(() => undefined);
    await vi.advanceTimersByTimeAsync(5);
    expect(heard).toEqual([]);
    // Calls 2 and 3 reconnect within 250 ms each: no storm.
    await vi.advanceTimersByTimeAsync(600);
    expect(heard).toEqual([]);
    // The long outage ends: one event.
    await vi.advanceTimersByTimeAsync(12_500);
    expect(heard).toEqual([ALL_DATA]);
    stop();
    off();
  });
});

describe("asking the desktop shell to confirm a person (crew_presence)", () => {
  type Win = { __TAURI_INTERNALS__?: { invoke: (command: string, args?: Record<string, unknown>) => unknown } };
  const win = window as unknown as Win;

  async function shell(attached: boolean, invoke?: (command: string, args?: Record<string, unknown>) => unknown) {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ attached, windows: 1, facts: {} }), { status: 200 })));
    win.__TAURI_INTERNALS__ = invoke ? { invoke } : undefined;
    await readShellStatus();
  }

  afterEach(() => {
    vi.unstubAllGlobals();
    win.__TAURI_INTERNALS__ = undefined;
  });

  it("is possible only in the desktop app, with the shell's bridge", async () => {
    await shell(true, vi.fn());
    expect(canAskPresence()).toBe(true);
    await shell(false, vi.fn());
    expect(canAskPresence()).toBe(false);
    await shell(true);
    expect(canAskPresence()).toBe(false);
  });

  it("calls that one command with the hold id and returns what it gave", async () => {
    const answer = Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join("");
    const invoke = vi.fn(async () => answer);
    await shell(true, invoke);
    await expect(askPresence("hold-1")).resolves.toBe(answer);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("crew_presence", { holdId: "hold-1" });
  });

  it("fails with the shell's own code, and never retries", async () => {
    const invoke = vi.fn(async () => { throw "presence_refused:gone"; });
    await shell(true, invoke);
    await expect(askPresence("hold-1")).rejects.toMatchObject({ code: "presence_refused:gone" });
    await expect(askPresence("hold-1")).rejects.toBeInstanceOf(PresenceFailure);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("is unavailable, without calling anything, outside the desktop app", async () => {
    const invoke = vi.fn();
    await shell(false, invoke);
    await expect(askPresence("hold-1")).rejects.toMatchObject({ code: "unavailable" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("refuses an answer that is not a nonce, and an error that is not a code", async () => {
    await shell(true, vi.fn(async () => ({ nonce: "x" })));
    await expect(askPresence("hold-1")).rejects.toMatchObject({ code: "invalid_answer" });
    await shell(true, vi.fn(async () => ""));
    await expect(askPresence("hold-1")).rejects.toMatchObject({ code: "invalid_answer" });
    await shell(true, vi.fn(async () => { throw new Error("boom"); }));
    await expect(askPresence("hold-1")).rejects.toMatchObject({ code: "unavailable" });
  });
});
