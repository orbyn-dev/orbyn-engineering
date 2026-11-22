import { afterEach, describe, expect, it, vi } from "vitest";
import { readSnapShotGate, installSnapshotTransport, type SnapShotTransport } from "./shell";
import { DEFAULT_SETTINGS, macState, takeOf } from "./snapshotFixture.test-support";

const calls: { command: string; args?: Record<string, unknown> }[] = [];
const listeners = new Map<string, Set<(payload: unknown) => void>>();

function transport(answer: (command: string, args?: Record<string, unknown>) => unknown): SnapShotTransport {
  return {
    async invoke(command, args) {
      calls.push({ command, args });
      return answer(command, args);
    },
    async listen(event, handler) {
      calls.push({ command: "listen", args: { event } });
      const set = listeners.get(event) ?? new Set();
      set.add(handler);
      listeners.set(event, set);
      return () => {
        calls.push({ command: "unlisten", args: { event } });
        set.delete(handler);
      };
    },
  };
}

afterEach(() => {
  installSnapshotTransport(null);
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  calls.length = 0;
  listeners.clear();
  vi.restoreAllMocks();
});

describe("the Snap Shot gate", () => {
  it("is hidden with no bridge and asks nothing, including the network", async () => {
    const fetch = vi.spyOn(globalThis, "fetch");
    expect(await readSnapShotGate()).toBeNull();
    expect(fetch).not.toHaveBeenCalled();
    expect(calls).toEqual([]);
  });

  it("is hidden when snapshot_state throws", async () => {
    installSnapshotTransport(transport(() => {
      throw new Error("no shell");
    }));
    expect(await readSnapShotGate()).toBeNull();
  });

  it("is hidden when the shell answers supported:false", async () => {
    installSnapshotTransport(transport(() => macState({ supported: false })));
    expect(await readSnapShotGate()).toBeNull();
  });

  it("is hidden when the answer is not a state", async () => {
    installSnapshotTransport(transport(() => ({ supported: "yes" })));
    expect(await readSnapShotGate()).toBeNull();
  });

  it("reads a supported state and keeps the bridge open for the feature", async () => {
    installSnapshotTransport(transport(() => macState({ supported: true })));
    const gate = await readSnapShotGate();
    expect(gate?.state.supported).toBe(true);
    expect(gate?.state.platform).toBe("macos");
  });
});

describe("the typed client", () => {
  it("maps every command to its contract name and argument", async () => {
    installSnapshotTransport(transport((command) => {
      if (command === "snapshot_state") return macState({ supported: true });
      if (command === "snapshot_capture") return { id: "snap-1" };
      if (command === "snapshot_take") return takeOf();
      return command === "snapshot_discard" ? null : macState({ supported: true, enabled: true });
    }));
    const gate = await readSnapShotGate();
    expect(gate).not.toBeNull();
    const client = gate!.client;

    await client.configure(DEFAULT_SETTINGS);
    expect(calls.at(-1)).toEqual({ command: "snapshot_configure", args: DEFAULT_SETTINGS });

    await client.capture();
    expect(calls.at(-1)).toEqual({ command: "snapshot_capture", args: undefined });

    await client.take("snap-1");
    expect(calls.at(-1)).toEqual({ command: "snapshot_take", args: { id: "snap-1" } });

    await client.discard("snap-1");
    expect(calls.at(-1)).toEqual({ command: "snapshot_discard", args: { id: "snap-1" } });

    await client.setup("test_capture");
    expect(calls.at(-1)).toEqual({ command: "snapshot_setup", args: { action: "test_capture" } });

    const stop = await client.on("snapshot://ready", vi.fn());
    expect(calls.at(-1)).toEqual({ command: "listen", args: { event: "snapshot://ready" } });
    stop();
    expect(calls.at(-1)).toEqual({ command: "unlisten", args: { event: "snapshot://ready" } });
  });
});
