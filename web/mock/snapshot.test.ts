import { afterEach, describe, expect, it, vi } from "vitest";
import { createSnapshotMock } from "./snapshot";
import { macState } from "../src/features/snapshot/snapshotFixture.test-support";

afterEach(() => vi.useRealTimers());

describe("the Snap Shot mock", () => {
  it("answers state, configure, capture, take, discard and setup", async () => {
    const mock = createSnapshotMock({ manual: true, state: macState({ supported: true }) });
    await expect(mock.invoke("snapshot_state")).resolves.toMatchObject({ supported: true });
    await expect(mock.invoke("snapshot_configure", { snapshot_enabled: true })).resolves.toMatchObject({ enabled: true });
    await expect(mock.invoke("snapshot_capture")).resolves.toEqual({ id: "snap-1" });
    await expect(mock.invoke("snapshot_take", { id: "snap-1" })).resolves.toMatchObject({ id: "snap-1", mime_type: "image/png" });
    await expect(mock.invoke("snapshot_discard", { id: "snap-1" })).resolves.toBeNull();
    await expect(mock.invoke("snapshot_setup", { action: "retry_shortcut" })).resolves.toMatchObject({ supported: true });
    expect(mock.calls.map((call) => call.command)).toEqual([
      "snapshot_state",
      "snapshot_configure",
      "snapshot_capture",
      "snapshot_take",
      "snapshot_discard",
      "snapshot_setup",
    ]);
  });

  it("emits every event to its listeners and stops after unlisten", async () => {
    const mock = createSnapshotMock();
    const heard: string[] = [];
    const stop = await mock.listen("snapshot://ready", (payload) => heard.push((payload as { id: string }).id));
    mock.emit("snapshot://ready", { id: "one" });
    expect(heard).toEqual(["one"]);
    stop();
    mock.emit("snapshot://ready", { id: "two" });
    expect(heard).toEqual(["one"]);
  });

  it("can leave a capture capturing until the test asks, or fail it", async () => {
    const mock = createSnapshotMock({ manual: true });
    await mock.invoke("snapshot_capture");
    expect(mock.emits.map((entry) => entry.event)).toEqual(["snapshot://started"]);
    mock.emit("snapshot://failed", { id: "snap-1", reason: "too_large" });
    expect(mock.emits.at(-1)).toEqual({ event: "snapshot://failed", payload: { id: "snap-1", reason: "too_large" } });
  });

  it("does not emit a late ready after a discard", async () => {
    vi.useFakeTimers();
    const mock = createSnapshotMock({ delayMs: 100 });
    const heard: string[] = [];
    await mock.listen("snapshot://ready", () => heard.push("ready"));
    await mock.invoke("snapshot_capture");
    await mock.invoke("snapshot_discard", { id: "snap-1" });
    await vi.advanceTimersByTimeAsync(200);
    expect(heard).toEqual([]);
  });
});
