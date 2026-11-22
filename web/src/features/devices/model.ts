import { HINGE_MAX, HINGE_MIN, type ActionType, type Device, type DeviceDetail, type DeviceState, type Platform, type Posture } from "./contract";

/** Text the service chose (a device name, a message): cut so no line can grow the panel. React escapes it; it is never markup. */
export const plain = (text: unknown, max = 80): string => {
  const whole = typeof text === "string" ? text : "";
  return whole.length > max ? `${whole.slice(0, max)}…` : whole;
};

const STATES = new Map<string, string>([
  ["shutdown", "Shut down"],
  ["booting", "Starting"],
  ["booted", "Running"],
  // Planned words the frames draw (device-frozen, device-crashed); the service contract has neither yet.
  ["frozen", "Not responding"],
  ["crashed", "Stopped"],
]);
export const stateLabel = (state: DeviceState | string): string => STATES.get(state) ?? "Unknown";

const PLATFORMS = new Map<string, string>([["android", "Android"], ["ios", "iOS"]]);
export const platformLabel = (platform: Platform | string): string => PLATFORMS.get(platform) ?? "Unknown";

const POSTURES = new Map<string, string>([["closed", "Closed"], ["half_opened", "Half open"], ["opened", "Open"], ["flipped", "Flipped"], ["tent", "Tent"]]);
/** The posture the service returned, in words. A word this app does not know is shown as it came, cut. Never assumed from an angle. */
export function postureLabel(posture: Posture | string | null | undefined): string {
  if (posture === null || posture === undefined) return "Not reported";
  return POSTURES.get(posture) ?? plain(posture, 24);
}

export function hingeLabel(angle: number | null | undefined): string {
  return typeof angle === "number" && Number.isFinite(angle) ? `${Math.round(angle)}°` : "not reported";
}

/** The widest PNG the contract allows is 16 MiB; as base64 that is ceil(16 MiB / 3) * 4 characters. */
export const MAX_IMAGE_CHARS = Math.ceil((16 * 1024 * 1024) / 3) * 4;
const PNG_PREFIX = "iVBORw0KGgo";

/** A screenshot as an image source, or `null` for anything that is not a PNG within the cap. The type is fixed here, never taken from the answer. */
export function screenshotSrc(image: unknown): string | null {
  if (typeof image !== "string" || image.length === 0 || image.length > MAX_IMAGE_CHARS) return null;
  return image.startsWith(PNG_PREFIX) && /^[A-Za-z0-9+/]+={0,2}$/.test(image) ? `data:image/png;base64,${image}` : null;
}

/** What one device's toolbar can do. Android and iOS differ (Eng 4's ruling); the service's own `actions` is the rest of the answer. */
export interface Controls {
  /** Back, Home and Recents: Android only. */
  buttons: boolean;
  rotate: boolean;
  screenshot: boolean;
  powerOff: boolean;
  fold: boolean;
  hinge: boolean;
  /** Controls the platform will get but cannot do yet, drawn disabled with a Planned pill. */
  planned: string[];
}

const has = (actions: readonly ActionType[], type: ActionType) => actions.includes(type);

/**
 * iOS: screenshot and power off only, never fold or hinge, and Rotate and Home are Planned. Android: what the service lists, with the
 * fold controls only on a device that is foldable and booted. Nothing is offered that the service did not list.
 */
export function controlsFor(device: Pick<Device, "platform" | "foldable" | "state">, actions: readonly ActionType[]): Controls {
  const booted = device.state === "booted";
  const android = device.platform === "android";
  const foldable = android && device.foldable && booted;
  return {
    buttons: android && booted && has(actions, "button"),
    rotate: android && booted && has(actions, "rotate"),
    screenshot: booted && has(actions, "screenshot"),
    powerOff: booted && has(actions, "power_off"),
    fold: foldable && has(actions, "fold") && has(actions, "unfold"),
    hinge: foldable && (has(actions, "hinge") || has(actions, "hinge_preset")),
    planned: device.platform === "ios" ? ["Rotate", "Home"] : [],
  };
}

export const clampAngle = (value: number): number => Math.min(HINGE_MAX, Math.max(HINGE_MIN, Math.round(value)));

/** The detail the panel shows: the one the service read back after a write replaces the list's copy. */
export const merged = (detail: DeviceDetail | null, device: Device): DeviceDetail | null => (detail && detail.id === device.id ? { ...detail, ...device } : detail);

/** A tab name is the person's own label: one line, cut, and never a device id or any markup. */
export const tabName = (name: unknown, fallback: string): string => {
  const label = typeof name === "string" ? name.trim() : "";
  const chosen = label === "" ? fallback : label;
  return plain(chosen, 60);
};

/** A vendor URL the tool steps may link to. Only http(s) is ever used as an href. */
export function safeHref(url: unknown): string | null {
  const text = typeof url === "string" ? url.trim() : "";
  return /^https?:\/\/[^\s<>"']+$/i.test(text) ? text : null;
}

/** A count of bytes for an install line, when the client sent one. */
export function installPercent(done: number, total: number): number {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.min(100, Math.max(0, Math.round((done / total) * 100)));
}
