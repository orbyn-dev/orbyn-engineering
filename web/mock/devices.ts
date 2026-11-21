/**
 * MOCK of the device routes of docs/devices.md (provisional), for tests and the dev-only preview (`#/preview/devices`), which install
 * it through `setDevicesApi`. The app's own client is the real HTTP one in src/features/devices/api.ts. Fixtures follow the Engineer
 * frames `device-fold-*` (Pixel Fold, Pixel 8, an iPhone). It refuses what the doc says the service refuses: `device_not_booted`,
 * `not_foldable`, `action_unsupported` (iOS rotate and buttons) and `device_busy`. The posture it answers for a hinge angle is the
 * mock's own guess (the doc leaves open what Android reports at 130); the panel must render whatever comes back.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { ActionResult, ActionType, Device, DeviceAction, DeviceDetail, DeviceList, DevicesApi, Fold, PlatformStatus } from "../src/features/devices/contract.ts";

/** A 1 by 1 PNG, as base64. */
export const PIXEL_PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

export function deviceFixtures(): Device[] {
  return [
    { id: "android:Pixel_Fold_API_35", name: "Pixel Fold", platform: "android", state: "booted", version: "15", foldable: true, fold: { posture: "opened", hinge_angle: 180 } },
    { id: "android:Pixel_8_API_35", name: "Pixel 8", platform: "android", state: "booted", version: "15", foldable: false },
    { id: "android:Pixel_9_Pro_Fold_API_35", name: "Pixel 9 Pro Fold", platform: "android", state: "shutdown", version: "15", foldable: true },
    { id: "ios:7C2D-0001", name: "iPhone 17 Pro", platform: "ios", state: "booted", version: "26.1", foldable: false },
  ];
}

export interface DevicesMock extends DevicesApi {
  /** Every write the panel made, in order: `<device id>:<type>` and the value, such as `android:Pixel_Fold_API_35:hinge:100`. */
  writes: string[];
  /** Make the next write fail with this code (`tool_failed`, `device_busy`, `device_not_found`, ...). */
  failNext(code: string | null): void;
  /** The posture the next fold-changing write answers, whatever the request was (the panel must show it). */
  answerPosture(next: Fold | null): void;
}

const STATUS: Record<string, [number, string]> = {
  device_not_found: [404, "That device is not in the list any more."],
  device_busy: [409, "Another action is running on this device."],
  tool_failed: [502, "The device tool did not finish."],
  not_foldable: [422, "This device does not fold."],
  action_unsupported: [422, "This device cannot do that."],
  device_not_booted: [409, "The device is not running."],
};

export function createDevicesMock(init: { devices?: Device[] | null; platforms?: PlatformStatus[]; delayMs?: number } = {}): DevicesMock {
  const missing = init.devices === null;
  const devices = new Map((init.devices ?? deviceFixtures()).map((device) => [device.id, device] as const));
  const platforms: PlatformStatus[] = init.platforms ?? [{ platform: "android", available: true }, { platform: "ios", available: true }];
  const writes: string[] = [];
  let fail: string | null = null;
  let posture: Fold | null = null;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const refuse = (code: string): never => {
    const [status, message] = STATUS[code] ?? [502, "The service could not do that."];
    throw new ApiFailure(code, message, status);
  };
  const find = (id: string): Device => devices.get(id) ?? refuse("device_not_found");
  const supported = (device: Device): ActionType[] => {
    if (device.platform === "ios") return ["screenshot", "power_off"];
    const base: ActionType[] = ["screenshot", "rotate", "button", "power_off"];
    return device.foldable ? [...base, "fold", "unfold", "hinge", "hinge_preset"] : base;
  };
  return {
    writes,
    failNext: (code) => { fail = code; },
    answerPosture: (next) => { posture = next; },
    async list() {
      await wait();
      if (missing) return null;
      return { devices: [...devices.values()], platforms } satisfies DeviceList;
    },
    async detail(id): Promise<DeviceDetail> {
      await wait();
      const device = find(id);
      return { ...device, actions: supported(device) };
    },
    async act(id, action: DeviceAction): Promise<ActionResult> {
      const value = "value" in action ? `:${action.value}` : "angle" in action ? `:${action.angle}` : "preset" in action ? `:${action.preset}` : "";
      writes.push(`${id}:${action.type}${value}`);
      await wait();
      if (fail !== null) {
        const code = fail;
        fail = null;
        refuse(code);
      }
      const device = find(id);
      if (!supported(device).includes(action.type)) refuse(device.foldable || !action.type.match(/fold|hinge/) ? "action_unsupported" : "not_foldable");
      if (device.state !== "booted") refuse("device_not_booted");
      if (action.type === "power_off") {
        const { fold: _gone, ...off } = device;
        devices.set(id, { ...off, state: "shutdown" });
        return { ok: true, device: devices.get(id)! };
      }
      if (action.type === "screenshot") return { ok: true, device, image: PIXEL_PNG };
      if (action.type === "fold" || action.type === "unfold" || action.type === "hinge" || action.type === "hinge_preset") {
        const angle = action.type === "fold" ? 0 : action.type === "unfold" ? 180 : action.type === "hinge" ? action.angle : { closed: 0, half_open: 130, open: 180 }[action.preset];
        const guess = angle === 0 ? "closed" : angle >= 180 ? "opened" : "half_opened";
        devices.set(id, { ...device, fold: posture ?? { posture: guess, hinge_angle: angle } });
        posture = null;
      }
      return { ok: true, device: devices.get(id)! };
    },
  };
}
