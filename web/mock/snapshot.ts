/**
 * MOCKS of the Snap Shot shell commands and events (docs/snapshots.md), for the tests and the dev-only preview
 * (`#/preview/snapshot`). The app has no capture code yet; this answers every command, can emit every event and sets the state
 * each design frame draws. It ships nowhere: the feature imports it only under `__ORBYN_PREVIEWS__`.
 */
import type { SnapShotMock } from "../src/features/snapshot/shell";
import type {
  SnapShotEventName,
  SnapShotFailureReason,
  SnapShotSettings,
  SnapShotSetupAction,
  SnapShotState,
  SnapShotTake,
} from "../src/features/snapshot/types";

type Timer = ReturnType<typeof setTimeout>;

export interface SnapshotMockOptions {
  state?: Partial<SnapShotState>;
  take?: Partial<SnapShotTake>;
  manual?: boolean;
  delayMs?: number;
  capture?: "ready" | "failed" | "excluded" | "pending";
  reason?: SnapShotFailureReason;
  appName?: string;
  /** Whether a setup allow_screen_recording reads back a grant. */
  grantScreenRecording?: boolean;
  grantAccessibility?: boolean;
  /** Whether the shortcut chord is owned by another app. */
  shortcutConflict?: boolean;
}

const takeBase = (patch: Partial<SnapShotTake> = {}): SnapShotTake => ({
  id: "snap-1",
  name: "Snap Shot · Safari.png",
  mime_type: "image/png",
  size_bytes: 2_400_000,
  source: {
    kind: "snap_shot",
    captured_at: "2026-11-22T14:32:08Z",
    app_name: "Safari",
    window_title: "Acme · Sign in",
    app_id: "com.apple.Safari",
    width: 1280,
    height: 800,
    ...(patch.source ?? {}),
  },
  png_base64: "aGVsbG8=",
  ...patch,
});

const baseState = (patch: Partial<SnapShotState> = {}): SnapShotState => {
  const base: SnapShotState = {
    supported: true,
    platform: "macos",
    enabled: false,
    shortcut_registered: true,
    shortcut_message: null,
    permissions: { screen_recording: false, accessibility: false },
    needs_setup: "screen_recording",
    message: null,
    ...patch,
  };
  return { ...base, permissions: { screen_recording: false, accessibility: false, ...(patch.permissions ?? {}) } };
};

const FAILURE_FRAMES: Record<string, SnapShotFailureReason> = {
  "snapshot-failed": "permission",
  "snapshot-failed-no-window": "no_window",
  "snapshot-failed-too-large": "too_large",
  "snapshot-failed-timeout": "timeout",
  "snapshot-failed-error": "capture_error",
  "snapshot-failed-narrow": "permission",
};

const GRANTED: Partial<SnapShotState> = { permissions: { screen_recording: true, accessibility: true }, needs_setup: null };

export function createSnapshotMock(options: SnapshotMockOptions = {}): SnapShotMock {
  const calls: { command: string; args?: Record<string, unknown> }[] = [];
  const configured: SnapShotSettings[] = [];
  const emits: { event: SnapShotEventName; payload?: unknown }[] = [];
  const listeners = new Map<SnapShotEventName, Set<(payload: unknown) => void>>();
  const timers = new Map<string, Timer>();
  const discarded = new Set<string>();
  const taken = new Set<string>();
  const settings: SnapShotSettings = {
    snapshot_enabled: false,
    snapshot_shortcut: { kind: "chord", key: "9", mod: true, shift: true, alt: false, ctrl: false },
    snapshot_include_accessibility: false,
    snapshot_play_sound: true,
    snapshot_sound: "soft-pop",
    snapshot_flash: true,
    snapshot_animations: true,
    snapshot_excluded_apps: [],
  };
  let state = baseState(options.state);
  if (options.shortcutConflict) state = { ...state, shortcut_registered: false, shortcut_message: "Another app uses Cmd+Shift+9." };
  let take = takeBase(options.take);
  let captureMode = options.capture ?? "ready";
  let reason: SnapShotFailureReason = options.reason ?? "capture_error";
  let appName = options.appName ?? "1Password";
  let counter = 0;

  const notify = (event: SnapShotEventName, payload?: unknown) => {
    emits.push(payload === undefined ? { event } : { event, payload });
    for (const handler of listeners.get(event) ?? []) handler(payload);
  };

  const emit = (event: SnapShotEventName, payload?: unknown) => {
    const id = typeof payload === "object" && payload !== null ? (payload as { id?: string }).id : undefined;
    if (id !== undefined && discarded.has(id)) return;
    notify(event, payload);
  };

  const finish = (id: string) => {
    timers.delete(id);
    if (discarded.has(id)) return;
    if (captureMode === "failed") {
      emit("snapshot://failed", { id, reason });
      return;
    }
    if (captureMode === "excluded") {
      emit("snapshot://excluded", { app_name: appName });
      return;
    }
    if (captureMode === "pending") return;
    emit("snapshot://ready", { id });
  };

  const start = (): string => {
    counter += 1;
    const id = `snap-${counter}`;
    emit("snapshot://started", { id });
    if (options.manual) return id;
    timers.set(id, setTimeout(() => finish(id), options.delayMs ?? 40));
    return id;
  };

  return {
    calls,
    configured,
    emits,
    setState(patch) {
      state = { ...state, ...patch, permissions: { ...state.permissions, ...(patch.permissions ?? {}) } };
    },
    setSettings(patch) {
      Object.assign(settings, patch);
    },
    setCapture(mode, detail = {}) {
      captureMode = mode;
      if (detail.reason !== undefined) reason = detail.reason as SnapShotFailureReason;
      if (detail.app_name !== undefined) appName = detail.app_name;
    },
    setTake(patch) {
      take = takeBase({ ...take, ...patch, source: { ...take.source, ...(patch.source ?? {}) } });
    },
    setFrame(frame) {
      if (frame === "snapshot-settings-linux") {
        state = baseState({ supported: false, enabled: false });
        return;
      }
      if (frame.includes("windows") || frame === "snapshot-setup-windows") {
        state = baseState({ platform: "windows", enabled: true, ...GRANTED });
      } else if (frame.endsWith("-off") || frame === "snapshot-settings-off-narrow") {
        state = baseState({ enabled: false });
      } else if (frame === "snapshot-settings-accessibility" || frame === "snapshot-setup-accessibility") {
        state = baseState({ enabled: true, permissions: { screen_recording: true, accessibility: false }, needs_setup: "accessibility" });
      } else if (frame === "snapshot-setup-denied" || frame === "snapshot-setup-narrow") {
        state = baseState({ enabled: true, permissions: { screen_recording: false, accessibility: false }, needs_setup: "screen_recording" });
      } else if (frame === "snapshot-setup-granted") {
        state = baseState({ enabled: true, ...GRANTED });
      } else if (frame === "snapshot-setup-asked") {
        state = baseState({ enabled: true, permissions: { screen_recording: false, accessibility: false }, needs_setup: "screen_recording" });
      } else if (frame === "snapshot-settings-conflict") {
        state = baseState({ enabled: true, ...GRANTED, shortcut_registered: false, shortcut_message: "Another app uses Cmd+Shift+9." });
      } else {
        state = baseState({ enabled: true, ...GRANTED });
      }
      captureMode = "ready";
      if (frame === "snapshot-capturing") captureMode = "pending";
      if (frame === "snapshot-excluded" || frame === "snapshot-excluded-narrow") {
        captureMode = "excluded";
        appName = "1Password";
      }
      reason = FAILURE_FRAMES[frame] ?? "capture_error";
      if (FAILURE_FRAMES[frame]) captureMode = "failed";
    },
    emit,
    async invoke(command, args) {
      calls.push(args === undefined ? { command } : { command, args });
      switch (command) {
        case "snapshot_state":
          return { ...state, permissions: { ...state.permissions } };
        case "snapshot_configure": {
          const sent = args as unknown as SnapShotSettings;
          configured.push(sent);
          Object.assign(settings, sent);
          state = { ...state, enabled: sent.snapshot_enabled };
          return { ...state, permissions: { ...state.permissions } };
        }
        case "snapshot_capture":
          return { id: start() };
        case "snapshot_take": {
          const id = String(args?.id ?? "");
          if (discarded.has(id) || taken.has(id)) throw new Error("not_found");
          taken.add(id);
          return { ...take, id, source: { ...take.source } };
        }
        case "snapshot_discard": {
          const id = String(args?.id ?? "");
          discarded.add(id);
          const timer = timers.get(id);
          if (timer) clearTimeout(timer);
          timers.delete(id);
          return null;
        }
        case "snapshot_setup": {
          const action = args?.action as SnapShotSetupAction;
          if (action === "retry_shortcut") {
            state = { ...state, shortcut_registered: !options.shortcutConflict, shortcut_message: options.shortcutConflict ? state.shortcut_message : null };
          } else if (action === "allow_screen_recording" && options.grantScreenRecording) {
            state = { ...state, permissions: { ...state.permissions, screen_recording: true } };
          } else if (action === "allow_accessibility" && options.grantAccessibility) {
            state = { ...state, permissions: { ...state.permissions, accessibility: true } };
          } else if (action === "test_capture") {
            start();
          }
          return { ...state, permissions: { ...state.permissions } };
        }
        case "plugin:event|listen":
          return 1;
        case "plugin:event|unlisten":
          return null;
        default:
          throw new Error(`unknown command ${command}`);
      }
    },
    async listen(event, handler) {
      const set = listeners.get(event) ?? new Set();
      set.add(handler);
      listeners.set(event, set);
      return () => set.delete(handler);
    },
  };
}

