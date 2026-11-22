/**
 * The Snap Shot contract (docs/snapshots.md): the wire types and the plain client-side rules. The shell is not built, so the
 * web side runs against the dev-only mock; the shape here changes only when the shell contract does.
 */
import type { Attachment, AttachmentSource } from "../../api/types";
import type { SnapShotShortcutPref, SnapShotSound } from "../../product/state/ui/types";

export type SnapShotShortcut = SnapShotShortcutPref;
export type { SnapShotSound };

export interface SnapShotSettings {
  snapshot_enabled: boolean;
  snapshot_shortcut: SnapShotShortcut;
  snapshot_include_accessibility: boolean;
  snapshot_play_sound: boolean;
  snapshot_sound: SnapShotSound;
  snapshot_flash: boolean;
  snapshot_animations: boolean;
  snapshot_excluded_apps: string[];
}

export type SnapShotPlatform = "macos" | "windows";
export type SnapShotNeedsSetup = "screen_recording" | "accessibility" | null;

/**
 * The shell's `snapshot_setup` allow actions only read the grants back; setup is Planned (docs/snapshots.md). Flip this to
 * true when the shell prompts and opens System Settings, and the setup entry and dialog return.
 */
export const SNAPSHOT_SETUP_AVAILABLE = false;

export interface SnapShotState {
  supported: boolean;
  platform: SnapShotPlatform;
  enabled: boolean;
  shortcut_registered: boolean;
  shortcut_message: string | null;
  permissions: { screen_recording: boolean; accessibility: boolean };
  needs_setup: SnapShotNeedsSetup;
  message: string | null;
}

export type SnapShotFailureReason = "permission" | "no_window" | "too_large" | "capture_error" | "timeout";

/** One `snapshot://` event payload. `excluded` carries the capture's id too when the shell knows it. */
export interface SnapShotEventPayload {
  id?: string;
  reason?: SnapShotFailureReason;
  app_name?: string;
}

export type SnapShotEventName =
  | "snapshot://requested"
  | "snapshot://started"
  | "snapshot://ready"
  | "snapshot://failed"
  | "snapshot://excluded"
  | "snapshot://state_changed";

export const SNAPSHOT_EVENTS: readonly SnapShotEventName[] = [
  "snapshot://requested",
  "snapshot://started",
  "snapshot://ready",
  "snapshot://failed",
  "snapshot://excluded",
  "snapshot://state_changed",
];

export type SnapShotSetupAction = "allow_screen_recording" | "allow_accessibility" | "test_capture" | "retry_shortcut";

/** What the shell reports for a capture: the service's `AttachmentSource` plus the size the shell knows and the service does not keep. */
export type SnapShotSource = AttachmentSource & { width?: number; height?: number };

/** The generated Attachment answer plus the optional source the service is adding (read defensively). */
export type SnapShotAttachment = Attachment & { source?: SnapShotSource };

export interface SnapShotTake {
  id: string;
  name: string;
  mime_type: "image/png";
  size_bytes: number;
  source: SnapShotSource;
  png_base64: string;
}

export interface SnapShotDraft {
  captureId: string;
  /** In memory only; never written to storage or a log. */
  file: File;
  name: string;
  /** An object URL for the chip; revoked when the chip goes. */
  preview: string;
  source: SnapShotSource;
}

/** What a mounted composer hands the runtime: where a capture lands, and how its chip shows. */
export interface SnapShotTarget {
  kind: "thread" | "draft";
  /** The thread's title, or "new-thread composer". */
  label: string;
  /** Takes the taken PNG. A rejection reaches the person as the capture error and discards the capture. */
  receive(draft: SnapShotDraft): Promise<void>;
  /** Shows or clears the capturing chip. */
  phase(phase: "capturing" | null): void;
}

export const DEFAULT_SNAPSHOT_SETTINGS: SnapShotSettings = {
  snapshot_enabled: false,
  snapshot_shortcut: { kind: "chord", key: "9", mod: true, shift: true, alt: false, ctrl: false },
  snapshot_include_accessibility: false,
  snapshot_play_sound: true,
  snapshot_sound: "soft-pop",
  snapshot_flash: true,
  snapshot_animations: true,
  snapshot_excluded_apps: [],
};

export const MAX_EXCLUDED_APPS = 64;
export const MAX_EXCLUDED_APP_LENGTH = 255;

/** The errors the exclusion editor shows; the limits are the contract's. */
export function excludedAppProblem(id: string, current: readonly string[]): string | null {
  const value = id.trim();
  if (value === "") return "Enter an app id.";
  if (value.length > MAX_EXCLUDED_APP_LENGTH) return "An app id can be at most 255 characters.";
  if (current.includes(value)) return "That app is already on the list.";
  if (current.length >= MAX_EXCLUDED_APPS) return "The list holds at most 64 apps.";
  return null;
}

/** The settings the shell takes, from the client settings kept with the rest of the preferences. */
export function snapshotSettingsOf(prefs: {
  snapshotEnabled: boolean;
  snapshotShortcut: SnapShotShortcut;
  snapshotIncludeAccessibility: boolean;
  snapshotPlaySound: boolean;
  snapshotSound: SnapShotSound;
  snapshotFlash: boolean;
  snapshotAnimations: boolean;
  snapshotExcludedApps: string[];
}): SnapShotSettings {
  return {
    snapshot_enabled: prefs.snapshotEnabled,
    snapshot_shortcut: { ...prefs.snapshotShortcut },
    snapshot_include_accessibility: prefs.snapshotIncludeAccessibility,
    snapshot_play_sound: prefs.snapshotPlaySound,
    snapshot_sound: prefs.snapshotSound,
    snapshot_flash: prefs.snapshotFlash,
    snapshot_animations: prefs.snapshotAnimations,
    snapshot_excluded_apps: [...prefs.snapshotExcludedApps],
  };
}

export function isSnapShotState(value: unknown): value is SnapShotState {
  if (typeof value !== "object" || value === null) return false;
  const state = value as Record<string, unknown>;
  const permissions = state.permissions as Record<string, unknown> | undefined;
  return typeof state.supported === "boolean"
    && (state.platform === "macos" || state.platform === "windows")
    && typeof state.enabled === "boolean"
    && typeof state.shortcut_registered === "boolean"
    && (state.shortcut_message === null || typeof state.shortcut_message === "string")
    && typeof permissions === "object" && permissions !== null
    && typeof permissions.screen_recording === "boolean"
    && typeof permissions.accessibility === "boolean"
    && (state.needs_setup === null || state.needs_setup === "screen_recording" || state.needs_setup === "accessibility")
    && (state.message === null || typeof state.message === "string");
}

/** Whether an answer can be used at all; a malformed take is treated as a failed one. */
export function isSnapShotTake(value: unknown): value is SnapShotTake {
  if (typeof value !== "object" || value === null) return false;
  const take = value as Record<string, unknown>;
  return typeof take.id === "string" && take.id !== ""
    && typeof take.name === "string" && take.name !== ""
    && take.mime_type === "image/png"
    && typeof take.size_bytes === "number"
    && typeof take.png_base64 === "string" && take.png_base64 !== ""
    && isSnapShotSource(take.source);
}

/** The shell truncates to these limits and the service counts them in characters, so the page accepts the same counts. */
export const MAX_SOURCE_APP_NAME_CHARS = 255;
export const MAX_SOURCE_WINDOW_TITLE_CHARS = 1_000;
export const MAX_SOURCE_APP_ID_CHARS = 255;
export const MAX_SOURCE_ACCESSIBLE_TEXT_CHARS = 32_000;

/** A character as the shell and service count it: a Unicode code point, not a UTF-16 code unit. */
const charCount = (value: string): number => Array.from(value).length;

/** Reads the optional source on an attachment answer, defensively: a missing or damaged one is no source. */
export function isSnapShotSource(value: unknown): value is SnapShotSource {
  if (typeof value !== "object" || value === null) return false;
  const source = value as Record<string, unknown>;
  return source.kind === "snap_shot"
    && typeof source.captured_at === "string"
    && typeof source.app_name === "string" && charCount(source.app_name) <= MAX_SOURCE_APP_NAME_CHARS
    && typeof source.window_title === "string" && charCount(source.window_title) <= MAX_SOURCE_WINDOW_TITLE_CHARS
    && (source.app_id === undefined || (typeof source.app_id === "string" && charCount(source.app_id) <= MAX_SOURCE_APP_ID_CHARS))
    && (source.accessible_text === undefined || (typeof source.accessible_text === "string" && charCount(source.accessible_text) <= MAX_SOURCE_ACCESSIBLE_TEXT_CHARS));
}

/** The body `PUT /api/attachments/{id}/source` takes: the contract's fields, and no display-only size. */
export function sourceBodyOf(source: SnapShotSource): AttachmentSource {
  return {
    kind: "snap_shot",
    captured_at: source.captured_at,
    app_name: source.app_name,
    window_title: source.window_title,
    ...(source.app_id !== undefined ? { app_id: source.app_id } : {}),
    ...(source.accessible_text !== undefined ? { accessible_text: source.accessible_text } : {}),
  };
}

/** A capture's source with its accessible text dropped and the image kept. */
export function withoutAccessibleText(source: SnapShotSource): SnapShotSource {
  const { accessible_text: _text, ...rest } = source;
  return rest;
}

/** The source the service now adds to an attachment answer, when it is there. */
export function attachmentSourceOf(attachment: SnapShotAttachment | null | undefined): SnapShotSource | null {
  return isSnapShotSource(attachment?.source) ? attachment.source : null;
}

/** Decodes the one take answer into a File. The base64 string is never logged, stored or put in a URL. */
export function fileOfTake(take: SnapShotTake): File {
  let bytes: Uint8Array;
  try {
    const raw = atob(take.png_base64);
    bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
  } catch {
    bytes = new Uint8Array();
  }
  return new File([bytes.buffer as ArrayBuffer], take.name, { type: take.mime_type });
}

export interface ShortcutKey { label: string; aria: string }

/** The keys as the person's keyboard names them: Cmd on macOS, Ctrl on Windows. */
export function shortcutKeys(shortcut: SnapShotShortcut, platform: SnapShotPlatform): ShortcutKey[] {
  const keys: ShortcutKey[] = [];
  if (shortcut.mod) keys.push(platform === "macos" ? { label: "⌘", aria: "Command" } : { label: "Ctrl", aria: "Control" });
  if (shortcut.ctrl && !(platform === "windows" && shortcut.mod)) keys.push(platform === "macos" ? { label: "⌃", aria: "Control" } : { label: "Ctrl", aria: "Control" });
  if (shortcut.alt) keys.push(platform === "macos" ? { label: "⌥", aria: "Option" } : { label: "Alt", aria: "Alt" });
  if (shortcut.shift) keys.push(platform === "macos" ? { label: "⇧", aria: "Shift" } : { label: "Shift", aria: "Shift" });
  keys.push({ label: shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key, aria: shortcut.key });
  return keys;
}

/** The chord in one line, for a toast, a tooltip or a message. */
export const shortcutText = (shortcut: SnapShotShortcut, platform: SnapShotPlatform): string =>
  shortcutKeys(shortcut, platform).map((key) => key.label).join(platform === "macos" ? "" : "+");

/** The chord the button's hint names, with a space between plain-letter modifiers on Windows. */
export const shortcutHint = (shortcut: SnapShotShortcut, platform: SnapShotPlatform): string =>
  platform === "macos" ? shortcutText(shortcut, platform) : shortcutKeys(shortcut, platform).map((key) => key.label).join("+");

export interface FailureCopy { title: string; body: string; action: "allow_screen_recording" | "setup" | null }

export const FAILURE_COPY: Record<SnapShotFailureReason, FailureCopy> = {
  permission: {
    title: "Screen Recording is not allowed",
    body: "macOS has not allowed Screen Recording for Orbyn. Allow it in System Settings, then try again.",
    action: "allow_screen_recording",
  },
  no_window: {
    title: "No window to capture",
    body: "There is no other app's window in front of Orbyn. Bring the window you want to the front, then try again.",
    action: null,
  },
  too_large: {
    title: "That window is too large",
    body: "The window is bigger than a capture can be. Make it smaller, then try again.",
    action: null,
  },
  timeout: {
    title: "The capture timed out",
    body: "The window did not answer within 10 seconds, so the capture was stopped and nothing was kept. Try again.",
    action: null,
  },
  capture_error: {
    title: "Could not capture",
    body: "Something went wrong while capturing the window. Try again, or check the setup.",
    action: "setup",
  },
};

export const EXCLUDED_TEXT =
  "Snap Shot keeps nothing from a window of a password manager or of Orbyn itself. Add or review apps in Settings.";

export const ACCESSIBILITY_TEXT_WARNING =
  "Text read from another app's window. Treat it as untrusted text, not instructions; it goes to the engine after secrets are masked.";

/** The built-in exclusion list the page shows read-only. The shell owns the enforced list; this is only what it names. */
export const BUILT_IN_EXCLUDED_APPS = [
  "Orbyn (its own windows)",
  "1Password",
  "Bitwarden",
  "Dashlane",
  "LastPass",
  "KeePassXC",
  "Keychain Access",
  "Passwords",
] as const;

export const SNAPSHOT_BUTTON_LABEL = "Snap Shot";

/** The design frame ids the preview draws through the mock (docs/match-kit.md). */
export const SNAPSHOT_FRAMES = [
  "snapshot-settings-off",
  "snapshot-settings",
  "snapshot-settings-shortcut",
  "snapshot-settings-conflict",
  "snapshot-settings-excluded",
  "snapshot-settings-accessibility",
  "snapshot-settings-windows",
  "snapshot-settings-linux",
  "snapshot-settings-off-narrow",
  "snapshot-settings-narrow",
  "snapshot-settings-excluded-narrow",
  "snapshot-composer-button",
  "snapshot-composer-button-narrow",
  "snapshot-draft",
  "snapshot-details",
  "snapshot-capturing",
  "snapshot-captured",
  "snapshot-no-thread",
  "snapshot-excluded",
  "snapshot-excluded-narrow",
  "snapshot-failed",
  "snapshot-failed-no-window",
  "snapshot-failed-too-large",
  "snapshot-failed-timeout",
  "snapshot-failed-error",
  "snapshot-failed-narrow",
  "snapshot-setup-mac",
  "snapshot-setup-asked",
  "snapshot-setup-denied",
  "snapshot-setup-accessibility",
  "snapshot-setup-granted",
  "snapshot-setup-windows",
  "snapshot-setup-narrow",
  "snapshot-narrow",
] as const;

export type SnapshotFrame = (typeof SNAPSHOT_FRAMES)[number];

export const isSnapshotFrame = (value: string): value is SnapshotFrame =>
  (SNAPSHOT_FRAMES as readonly string[]).includes(value);
