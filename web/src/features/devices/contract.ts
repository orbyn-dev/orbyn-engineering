/**
 * The device contract of docs/devices.md, answered by the service (#1250; not yet run against a real adb or simctl by the web).
 * Hand-written so the frames' extra states (`frozen`, `crashed`) can sit beside the service's two; `generatedContract.test.ts`
 * holds every other field to the generated types in web/src/generated/Device*.ts.
 */

export type Platform = "android" | "ios";
/**
 * The service answers `booted` or `shutdown` only; `booting` is never sent and is not mapped. `frozen` and `crashed`
 * are the target states the frames draw (device-frozen, device-crashed) and the mock sends them for those frames;
 * a word the service adds later is shown as Unknown.
 */
export type DeviceState = "shutdown" | "booted" | "frozen" | "crashed";
/** The device's current orientation in the contract's order. Only when the device is booted and the tool reports it. */
export type Orientation = "portrait" | "landscape_left" | "portrait_upside_down" | "landscape_right";
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
  /** The orientation the service read, when the device is booted and the tool reports it; absent when unreadable. */
  orientation?: Orientation;
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

/** `{ok: true, device}` with the state read back; a screenshot also answers `image`, a PNG as base64. */
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
  /** A write. Person-class (`PERSON_ROUTES`): a key-less client is answered 403 `person_required`, a shell-less service 403 `desktop_required`. */
  act(id: string, action: DeviceAction): Promise<ActionResult>;
}
