import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSnapshotMock } from "../../../mock/snapshot";
import { useUi } from "../../product/state/ui";
import { installSnapshotTransport } from "./shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "./runtime";
import { DEFAULT_SETTINGS, recordingTarget, takeOf } from "./snapshotFixture.test-support";

let mock: ReturnType<typeof createSnapshotMock>;

beforeEach(() => {
  mock = createSnapshotMock({ manual: true });
  installSnapshotTransport(mock);
});

afterEach(() => {
  resetSnapshotRuntimeForTests();
  installSnapshotTransport(null);
  vi.useRealTimers();
});

const start = async () => {
  await useSnapshot.getState().start();
};

describe("the feature gate in the runtime", () => {
  it("hides everything with no bridge and sends no request", async () => {
    installSnapshotTransport(null);
    const fetch = vi.spyOn(globalThis, "fetch");
    await start();
    expect(useSnapshot.getState().status).toBe("hidden");
    expect(fetch).not.toHaveBeenCalled();
    expect(mock.calls).toEqual([]);
  });

  it("hides everything when snapshot_state throws", async () => {
    installSnapshotTransport({
      async invoke() {
        throw new Error("not this build");
      },
      async listen() {
        return () => undefined;
      },
    });
    await start();
    expect(useSnapshot.getState().status).toBe("hidden");
    expect(useSnapshot.getState().state).toBeNull();
  });

  it("hides everything when the shell answers supported:false", async () => {
    mock.setState({ supported: false });
    await start();
    expect(useSnapshot.getState().status).toBe("hidden");
  });
});

describe("settings", () => {
  it("sends the full settings once on start, with the contract defaults", async () => {
    await start();
    expect(useSnapshot.getState().status).toBe("ready");
    expect(mock.configured).toHaveLength(1);
    expect(mock.configured[0]).toEqual(DEFAULT_SETTINGS);
  });

  it("sends the full settings on every change", async () => {
    await start();
    const changes = [
      [{ snapshotEnabled: true }, { snapshot_enabled: true }],
      [{ snapshotIncludeAccessibility: true }, { snapshot_include_accessibility: true }],
      [{ snapshotPlaySound: false }, { snapshot_play_sound: false }],
      [{ snapshotSound: "camera-shutter" as const }, { snapshot_sound: "camera-shutter" }],
      [{ snapshotFlash: false }, { snapshot_flash: false }],
      [{ snapshotAnimations: false }, { snapshot_animations: false }],
      [{ snapshotExcludedApps: ["com.example.vault"] as string[] }, { snapshot_excluded_apps: ["com.example.vault"] }],
      [{ snapshotShortcut: { kind: "chord" as const, key: "k", mod: true, shift: true, alt: false, ctrl: false } }, { snapshot_shortcut: { kind: "chord", key: "k", mod: true, shift: true, alt: false, ctrl: false } }],
    ] as const;
    for (const [change, expected] of changes) {
      const before = mock.configured.length;
      useUi.getState().setPrefs(change);
      await vi.waitFor(() => expect(mock.configured.length).toBeGreaterThan(before));
      const sent = mock.configured.at(-1)!;
      expect(Object.keys(sent).sort()).toEqual(Object.keys(DEFAULT_SETTINGS).sort());
      expect(sent).toMatchObject(expected);
    }
  });

  it("shows the shell's answer, never its own guess", async () => {
    mock.setState({ shortcut_registered: false, shortcut_message: "Another app uses Cmd+Shift+9." });
    await start();
    expect(useSnapshot.getState().state?.shortcut_registered).toBe(false);
    expect(useSnapshot.getState().state?.shortcut_message).toBe("Another app uses Cmd+Shift+9.");
  });
});

describe("capturing", () => {
  it("starts only from the button or the requested event", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    mock.emit("snapshot://started", { id: "stray" });
    mock.emit("snapshot://state_changed");
    expect(mock.calls.some((call) => call.command === "snapshot_capture")).toBe(false);

    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    mock.emit("snapshot://requested");
    await vi.waitFor(() => expect(mock.calls.some((call) => call.command === "snapshot_capture")).toBe(true));
  });

  it("starts one capture when the button is clicked twice before the shell answers", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await Promise.all([
      useSnapshot.getState().captureFromPerson(),
      useSnapshot.getState().captureFromPerson(),
    ]);
    expect(mock.calls.filter((call) => call.command === "snapshot_capture")).toHaveLength(1);
  });

  it("does not capture while the setting is off", async () => {
    await start();
    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    mock.emit("snapshot://requested");
    await Promise.resolve();
    expect(mock.calls.some((call) => call.command === "snapshot_capture")).toBe(false);
  });

  it("takes the capture on ready and hands it to the target", async () => {
    mock.setTake(takeOf());
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered, phases } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    const id = useSnapshot.getState().capture?.id;
    expect(id).toBeTruthy();
    mock.emit("snapshot://ready", { id });
    await vi.waitFor(() => expect(delivered).toHaveLength(1));
    expect(mock.calls.map((call) => call.command)).toEqual(
      expect.arrayContaining(["snapshot_capture", "snapshot_take"]),
    );
    expect(delivered[0]).toMatchObject({ name: "Snap Shot · Safari.png", source: expect.objectContaining({ app_name: "Safari" }) });
    expect(phases.at(-1)).toBeNull();
    expect(useUi.getState().toasts.at(-1)?.text).toBe(
      "Captured Safari · it is a draft in Fix login redirect. Nothing is sent until you send it.",
    );
    expect(useUi.getState().toasts.at(-1)?.kind).toBe("done");
  });

  it("ignores late events for a discarded id", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    const id = useSnapshot.getState().capture!.id;
    await useSnapshot.getState().discard(id);
    const toasts = useUi.getState().toasts.length;
    mock.emit("snapshot://ready", { id });
    mock.emit("snapshot://failed", { id, reason: "capture_error" });
    mock.emit("snapshot://excluded", { id, app_name: "1Password" });
    await Promise.resolve();
    expect(useUi.getState().toasts).toHaveLength(toasts);
    expect(delivered).toEqual([]);
    expect(useSnapshot.getState().capture).toBeNull();
    expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id } });
  });

  it("settles a failure that arrives before snapshot_capture answers the id", async () => {
    const base = createSnapshotMock({ manual: true });
    installSnapshotTransport({
      async invoke(command, args) {
        if (command === "snapshot_capture") {
          base.emit("snapshot://started", { id: "snap-early" });
          base.emit("snapshot://failed", { id: "snap-early", reason: "permission" });
          return { id: "snap-early" };
        }
        return base.invoke(command, args);
      },
      listen: (event, handler) => base.listen(event, handler),
    });
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, phases } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    expect(useSnapshot.getState().capture).toBeNull();
    expect(phases.at(-1)).toBeNull();
    expect(useUi.getState().toasts.at(-1)?.text).toContain("Screen Recording is not allowed");
    expect(useUi.getState().toasts.at(-1)?.kind).toBe("failed");
  });

  it("replays a ready that arrived before snapshot_capture answered the id", async () => {
    const base = createSnapshotMock({ manual: true, take: takeOf() });
    installSnapshotTransport({
      async invoke(command, args) {
        if (command === "snapshot_capture") {
          base.emit("snapshot://started", { id: "snap-early" });
          base.emit("snapshot://ready", { id: "snap-early" });
          return { id: "snap-early" };
        }
        return base.invoke(command, args);
      },
      listen: (event, handler) => base.listen(event, handler),
    });
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered, phases } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    await vi.waitFor(() => expect(delivered).toHaveLength(1));
    expect(delivered[0]).toMatchObject({ captureId: "snap-early", source: expect.objectContaining({ app_name: "Safari" }) });
    expect(useSnapshot.getState().capture).toBeNull();
    expect(phases.at(-1)).toBeNull();
  });

  it("offers Screen Recording setup from a permission failure now that setup is built", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    mock.emit("snapshot://failed", { id: useSnapshot.getState().capture!.id, reason: "permission" });
    await vi.waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain("Screen Recording is not allowed"));
    expect(useUi.getState().toasts.at(-1)?.action?.label).toBe("Allow Screen Recording");
  });

  it("accepts the test capture's own id, which snapshot_capture did not return", async () => {
    mock.setTake(takeOf());
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().testCapture();
    expect(mock.calls.at(-1)).toEqual({ command: "snapshot_setup", args: { action: "test_capture" } });
    mock.emit("snapshot://started", { id: "test-1" });
    mock.emit("snapshot://ready", { id: "test-1" });
    await vi.waitFor(() => expect(delivered).toHaveLength(1));
    expect(useSnapshot.getState().testCaptureWorked).toBe(true);
  });

  it("flashes for 120 ms when a capture is ready", async () => {
    vi.useFakeTimers();
    mock.setTake(takeOf());
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    const id = useSnapshot.getState().capture!.id;
    mock.emit("snapshot://ready", { id });
    await vi.waitFor(() => expect(useSnapshot.getState().flashing).toBe(true));
    await vi.advanceTimersByTimeAsync(120);
    expect(useSnapshot.getState().flashing).toBe(false);
  });
});

describe("failures and exclusions", () => {
  const cases = [
    ["permission", "Screen Recording is not allowed"],
    ["no_window", "No window to capture"],
    ["too_large", "That window is too large"],
    ["timeout", "The capture timed out"],
    ["capture_error", "Could not capture"],
  ] as const;

  it.each(cases)("says what a %s failure means", async (reason, title) => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    const id = useSnapshot.getState().capture!.id;
    mock.emit("snapshot://failed", { id, reason });
    await vi.waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain(title));
    expect(useUi.getState().toasts.at(-1)?.kind).toBe("failed");
    expect(delivered).toEqual([]);
    expect(useSnapshot.getState().capture).toBeNull();
  });

  it("clears the capture only for an excluded event that names it", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, phases } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    const id = useSnapshot.getState().capture!.id;
    const toasts = useUi.getState().toasts.length;
    mock.emit("snapshot://excluded", { id: "another-capture", app_name: "1Password" });
    await Promise.resolve();
    expect(useSnapshot.getState().capture?.id).toBe(id);
    expect(phases.at(-1)).toBe("capturing");
    expect(useUi.getState().toasts).toHaveLength(toasts);

    mock.emit("snapshot://excluded", { id, app_name: "1Password" });
    await vi.waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain("Not captured: 1Password"));
    expect(useSnapshot.getState().capture).toBeNull();
    expect(phases.at(-1)).toBeNull();
  });

  it("names only the excluded app, never the window", async () => {
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target } = recordingTarget();
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    mock.emit("snapshot://excluded", { app_name: "1Password", window_title: "Private vault" });
    await vi.waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain("Not captured: 1Password"));
    expect(useUi.getState().toasts.at(-1)?.text).not.toContain("Private vault");
  });
});

describe("the no-thread composer", () => {
  it("says the capture went to the new-thread composer", async () => {
    mock.setTake(takeOf());
    await start();
    useUi.getState().setPrefs({ snapshotEnabled: true });
    await vi.waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(true));
    const { target, delivered } = recordingTarget("draft", "new-thread composer");
    useSnapshot.getState().registerTarget(target);
    await useSnapshot.getState().captureFromPerson();
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await vi.waitFor(() => expect(delivered).toHaveLength(1));
    expect(useUi.getState().toasts.at(-1)?.text).toBe(
      "Captured Safari · no thread is open, so it is a draft in the new-thread composer. Nothing is sent.",
    );
  });
});
