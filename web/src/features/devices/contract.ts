/**
 * The device contract of docs/devices.md (PROVISIONAL: Eng 4's contract, not yet run against adb or simctl, and no service
 * answers it yet). Hand-written because no generated types exist for these routes. Field names are the doc's; anything the
 * doc leaves open is marked "guess" and listed in the pull request.
 */

export type Platform = "android" | "ios";
/**
 * Eng 4's ruling: the service answers `booted` or `shutdown`. `booting`, `frozen` and `crashed` are the target
 * states the frames draw (device-booting, device-frozen, device-crashed); the mock sends them for those frames,
 * and a word the service adds later is shown as Unknown.
 */
export type DeviceState = "shutdown" | "booting" | "booted" | "frozen" | "crashed";
/** `flipped` and `tent` are read only. The web renders whatever posture the service returns and never assumes one. */
export type Posture = "closed" | "half_opened" | "opened" | "flipped" | "tent";

export interface Fold {
  /** `null` when the tool reports none. */
  posture: Posture | null;
  /** Degrees, or `null` when the tool does not report it. */
  hinge_angle: number | null;
}

export interface Device {
  /** `android:<avd>` or `ios:<udid>`. Only ever taken from the list the service gave. */
  id: string;
  name: string;
  platform: Platform;
  state: DeviceState;
  version: string;
  foldable: boolean;
  /** Only when `foldable` and the device is booted. */
  fold?: Fold;
}

export interface PlatformStatus {
  platform: Platform;
  available: boolean;
  code?: string;
  message?: string;
}

export interface DeviceList {
  devices: Device[];
  platforms: PlatformStatus[];
}

export type ActionType = "screenshot" | "rotate" | "button" | "power_off" | "fold" | "unfold" | "hinge" | "hinge_preset";
export type Orientation = "portrait" | "landscape_left" | "portrait_upside_down" | "landscape_right";
export type ButtonName = "home" | "back" | "recents";
export type HingePreset = "closed" | "half_open" | "open";

/** The typed body of `POST /api/devices/{id}/actions`. The body never carries a device id. */
export type DeviceAction =
  | { type: "screenshot" }
  | { type: "rotate"; value: Orientation }
  | { type: "button"; value: ButtonName }
  | { type: "power_off" }
  | { type: "fold" }
  | { type: "unfold" }
  | { type: "hinge"; angle: number }
  | { type: "hinge_preset"; preset: HingePreset };

/** `GET /api/devices/{id}`: the device and the action types it supports right now. */
export interface DeviceDetail extends Device {
  actions: ActionType[];
}

/** `{ok: true, device}` with the state read back; a screenshot also answers `image`, a PNG as base64 (guess: the field name is the doc's). */
export interface ActionResult {
  ok: true;
  device: Device;
  image?: string;
}

/** The hinge's bounds (provisional until adb confirms). Closed and Open sit outside it: only a preset, Fold or Unfold reaches them. */
export const HINGE_MIN = 1;
export const HINGE_MAX = 179;

export interface DevicesApi {
  /** The device list, or `null` when the service has no devices route (`404`): the feature is hidden, with no error. */
  list(signal?: AbortSignal): Promise<DeviceList | null>;
  detail(id: string, signal?: AbortSignal): Promise<DeviceDetail>;
  /** A write. Person-class (`PENDING_PERSON_ROUTES`). */
  act(id: string, action: DeviceAction): Promise<ActionResult>;
}
