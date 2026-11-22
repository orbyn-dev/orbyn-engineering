import type { SnapShotSettings, SnapShotSource, SnapShotState, SnapShotTake } from "./types";

export const SETTINGS_KEYS = [
  "snapshot_enabled",
  "snapshot_shortcut",
  "snapshot_include_accessibility",
  "snapshot_play_sound",
  "snapshot_sound",
  "snapshot_flash",
  "snapshot_animations",
  "snapshot_excluded_apps",
] as const;

export const DEFAULT_SETTINGS: SnapShotSettings = {
  snapshot_enabled: false,
  snapshot_shortcut: { kind: "chord", key: "9", mod: true, shift: true, alt: false, ctrl: false },
  snapshot_include_accessibility: false,
  snapshot_play_sound: true,
  snapshot_sound: "soft-pop",
  snapshot_flash: true,
  snapshot_animations: true,
  snapshot_excluded_apps: [],
};

export const macState = (patch: Partial<SnapShotState> = {}): SnapShotState => ({
  supported: true,
  platform: "macos",
  enabled: false,
  shortcut_registered: true,
  shortcut_message: null,
  permissions: { screen_recording: false, accessibility: false },
  needs_setup: "screen_recording",
  message: null,
  ...patch,
});

export const sourceOf = (patch: Partial<SnapShotSource> = {}): SnapShotSource => ({
  kind: "snap_shot",
  captured_at: "2026-11-22T14:32:08Z",
  app_name: "Safari",
  window_title: "Acme · Sign in",
  app_id: "com.apple.Safari",
  width: 1280,
  height: 800,
  ...patch,
});

export const takeOf = (patch: Partial<SnapShotTake> = {}): SnapShotTake => ({
  id: "snap-1",
  name: "Snap Shot · Safari.png",
  mime_type: "image/png",
  size_bytes: 42,
  source: sourceOf(),
  png_base64: "aGVsbG8=",
  ...patch,
});

/** A target that records what the runtime delivered. */
export function recordingTarget(kind: "thread" | "draft" = "thread", label = "Fix login redirect") {
  const delivered: unknown[] = [];
  const phases: Array<"capturing" | null> = [];
  return {
    target: {
      kind,
      label,
      async receive(draft: unknown) {
        delivered.push(draft);
      },
      phase(phase: "capturing" | null) {
        phases.push(phase);
      },
    },
    delivered,
    phases,
  };
}
