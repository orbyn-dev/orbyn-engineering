/**
 * MOCK of the device routes of docs/devices.md (provisional), for tests and the dev-only preview (`#/preview/devices`), which install
 * it through `setDevicesApi`. The app's own client is the real HTTP one in src/features/devices/api.ts. Fixtures follow the
 * Device frames: the list, tabs, states and setup screens. It refuses what the doc says the service refuses: `device_not_booted`,
 * `not_foldable`, `action_unsupported` (iOS rotate and buttons) and `device_busy`. The posture it answers for a hinge angle is the
 * mock's own guess (the doc leaves open what Android reports at 130); the panel must render whatever comes back.
 *
 * `createDevicesPlannedMock` covers the pieces docs/devices.md does not define (tools, their versions, the one Orbyn install and
 * the agent-access toggle). The production client for those answers nothing; only this mock makes the setup frames interactive.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { ActionResult, ActionType, Device, DeviceAction, DeviceDetail, DeviceList, DeviceState, DevicesApi, Fold, PlatformStatus } from "../src/features/devices/contract.ts";
import type { DeviceTool, DevicesPlannedApi, InstallProgress, ToolSnapshot, ToolVersions } from "../src/features/devices/planned.ts";

/** A 1 by 1 PNG, as base64. */
export const PIXEL_PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

export function deviceFixtures(): Device[] {
  return [
    { id: "android:Pixel_Fold_API_35", name: "Pixel Fold", platform: "android", state: "booted", version: "15", foldable: true, orientation: "portrait", fold: { posture: "opened", hinge_angle: 180 } },
    { id: "android:Pixel_8_API_35", name: "Pixel 8", platform: "android", state: "booted", version: "15", foldable: false, orientation: "portrait" },
    { id: "android:Pixel_9_Pro_Fold_API_35", name: "Pixel 9 Pro Fold", platform: "android", state: "shutdown", version: "15", foldable: true },
    { id: "ios:7C2D-0001", name: "iPhone 17 Pro", platform: "ios", state: "booted", version: "26.1", foldable: false, orientation: "portrait" },
  ];
}

/** The state frames' own devices (device-running, device-frozen, device-crashed, device-shutdown). */
export function stateFixture(state: DeviceState, platform: "android" | "ios" = "ios"): Device {
  return {
    id: platform === "ios" ? "ios:7C2D-0001" : "android:Pixel_8_API_35",
    name: platform === "ios" ? "iPhone 16" : "Pixel 8",
    platform,
    state,
    version: platform === "ios" ? "18.2" : "15",
    foldable: false,
    ...(state === "booted" ? { orientation: "portrait" as const } : {}),
  };
}

export interface DevicesMock extends DevicesApi {
  /** Every write the panel made, in order: `<device id>:<type>` and the value, such as `android:Pixel_Fold_API_35:hinge:100`. */
  writes: string[];
  /** Make the next write fail with this code (`tool_failed`, `device_busy`, `device_not_found`, ...). */
  failNext(code: string | null): void;
  /** Make the next detail read fail with this code, without touching the list. */
  failNextDetail(code: string | null): void;
  /** Replace a listed device, as a state frame needs. */
  setDevice(device: Device): void;
  /** The posture the next fold-changing write answers, whatever the request was (the panel must show it). */
  answerPosture(next: Fold | null): void;
}

const STATUS: Record<string, [number, string]> = {
  invalid_action: [400, "That action is not valid."],
  device_not_found: [404, "That device is no longer there."],
  device_busy: [409, "Another action is running on this device."],
  tool_missing: [409, "Android tools were not found. Install Android platform-tools and the emulator."],
  device_not_booted: [409, "Start the device first."],
  not_foldable: [422, "This device does not fold."],
  action_unsupported: [422, "This device cannot do that."],
  tool_failed: [502, "The device tool could not do that. Try again."],
  tool_timeout: [504, "The device did not answer in time."],
  tool_output_too_large: [413, "The device sent more than Orbyn can read."],
};

const TOOL_MISSING = {
  android: "Android tools were not found. Install Android platform-tools and the emulator.",
  ios: "Xcode was not found. Install Xcode, then try again.",
} as const;

const refuse = (code: string): never => {
  const [status, message] = STATUS[code] ?? [502, "The service could not do that."];
  throw new ApiFailure(code, message, status);
};

export function createDevicesMock(init: { devices?: Device[] | null; platforms?: PlatformStatus[]; delayMs?: number } = {}): DevicesMock {
  const missing = init.devices === null;
  const devices = new Map((init.devices ?? deviceFixtures()).map((device) => [device.id, device] as const));
  const platforms: PlatformStatus[] = init.platforms ?? [{ platform: "android", available: true }, { platform: "ios", available: true }];
  const writes: string[] = [];
  let fail: string | null = null;
  let detailFail: string | null = null;
  let posture: Fold | null = null;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const find = (id: string): Device => devices.get(id) ?? refuse("device_not_found");
  const supported = (device: Device): ActionType[] => {
    if (device.platform === "ios") return ["screenshot", "power_off"];
    const base: ActionType[] = ["screenshot", "rotate", "button", "power_off"];
    return device.foldable ? [...base, "fold", "unfold", "hinge", "hinge_preset"] : base;
  };
  return {
    writes,
    failNext: (code) => { fail = code; },
    failNextDetail: (code) => { detailFail = code; },
    setDevice: (device) => { devices.set(device.id, device); },
    answerPosture: (next) => { posture = next; },
    async list() {
      await wait();
      if (missing) return null;
      return { devices: [...devices.values()], platforms } satisfies DeviceList;
    },
    async detail(id): Promise<DeviceDetail> {
      await wait();
      if (detailFail !== null) {
        const code = detailFail;
        detailFail = null;
        refuse(code);
      }
      const device = find(id);
      return { ...device, actions: supported(device) };
    },
    async act(id, action: DeviceAction): Promise<ActionResult> {
      const value = "value" in action ? `:${action.value}` : "angle" in action ? `:${action.angle}` : "preset" in action ? `:${action.preset}` : "";
      writes.push(`${id}:${action.type}${value}`);
      await wait();
      const device = find(id);
      if (fail !== null) {
        const code = fail;
        fail = null;
        if (code === "tool_missing") throw new ApiFailure(code, TOOL_MISSING[device.platform], 409);
        refuse(code);
      }
      if (!supported(device).includes(action.type)) refuse(device.foldable || !action.type.match(/fold|hinge/) ? "action_unsupported" : "not_foldable");
      if (device.state !== "booted") refuse("device_not_booted");
      if (action.type === "power_off") {
        const { fold: _gone, orientation: _unread, ...off } = device;
        devices.set(id, { ...off, state: "shutdown" });
        return { ok: true, device: devices.get(id)! };
      }
      if (action.type === "screenshot") return { ok: true, device, image: PIXEL_PNG };
      if (action.type === "rotate") {
        devices.set(id, { ...device, orientation: action.value });
        return { ok: true, device: devices.get(id)! };
      }
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

/* ---------------------------------------------------------------------------------------------------------------- */
/* Planned pieces: docs/devices.md does not define tools, versions, the agent-device install or the access toggle.  */
/* ---------------------------------------------------------------------------------------------------------------- */

const STEPS = {
  xcode: {
    title: "Install Xcode",
    text: "Install Xcode from the App Store, then open it once and accept its licence.",
    url: "https://developer.apple.com/xcode/",
    domain: "developer.apple.com",
  },
  runtime: {
    title: "How to update the iOS Simulator runtime (example)",
    text: "Open Xcode › Settings › Components, choose iOS 18.4 and select Get. Or run:",
    command: "xcodebuild -downloadPlatform iOS",
    url: "https://developer.apple.com/documentation/xcode/installing-additional-simulator-runtimes",
    domain: "developer.apple.com",
  },
  android: {
    title: "Steps from the Android developers",
    text: "Install with Android Studio’s SDK Manager, or run:",
    command: 'sdkmanager "platform-tools" "emulator" "system-images;android-35;google_apis;arm64-v8a"',
    url: "https://developer.android.com/studio",
    domain: "developer.android.com",
  },
  agent: {
    title: "agent-device",
    text: "Lets an agent tap, type and read the screen. Optional; installs into Orbyn’s own folder.",
  },
} as const;

function iosTools(missing = false): DeviceTool[] {
  return [
    { id: "xcode", name: "Xcode command line tools", required: true, state: missing ? "missing" : "ready", version: missing ? null : "16.2", latest: missing ? null : "16.2", note: missing ? "Not found on this computer" : "Version 16.2 · found at /Library/Developer", steps: STEPS.xcode },
    { id: "ios-runtime", name: "iOS Simulator runtime", required: true, state: "ready", version: "18.2", latest: "18.2", note: missing ? "Needs Xcode first" : "iOS 18.2 · 7.9 GB", steps: STEPS.runtime },
    { id: "agent-device", name: "agent-device", required: false, state: "missing", version: null, latest: "0.9.3", note: "Installs into Orbyn’s own folder", steps: STEPS.agent },
  ];
}

function androidTools(missing = true): DeviceTool[] {
  return [
    { id: "platform-tools", name: "Platform tools (adb)", required: true, state: missing ? "missing" : "ready", version: missing ? null : "35.0.2", latest: "35.0.2", note: missing ? "Not found on this computer" : "Version 35.0.2", steps: STEPS.android },
    { id: "emulator", name: "Android Emulator", required: true, state: missing ? "missing" : "ready", version: missing ? null : "35.1.4", latest: "35.1.4", note: missing ? "Not found on this computer" : "Version 35.1.4", steps: STEPS.android },
    { id: "system-image", name: "System image: Pixel 8, Android 15", required: true, state: missing ? "missing" : "ready", version: missing ? null : "35", latest: "35", note: missing ? "Not found on this computer" : "Android 15 · arm64-v8a", steps: STEPS.android },
    { id: "agent-device", name: "agent-device", required: false, state: "missing", version: null, latest: "0.9.3", note: "Installs into Orbyn’s own folder", steps: STEPS.agent },
  ];
}

export interface DevicesPlannedMock extends DevicesPlannedApi {
  /** How many times a version check ran. */
  checks: number;
  /** Every agent-access write, in order. */
  accessWrites: boolean[];
  /** Every restart request, by device id. */
  restarts: string[];
  /** How many agent-device installs started. */
  installs: number;
  /** Make the next install fail with the frame's connection-reset line. */
  failInstall(on: boolean): void;
  setOffline(offline: boolean): void;
  setTools(platform: "android" | "ios", tools: DeviceTool[]): void;
}

/** The setup and tool-version client the frames draw. Nothing here is a production route. */
export function createDevicesPlannedMock(init: {
  delayMs?: number;
  offline?: boolean;
  access?: boolean;
  iosMissing?: boolean;
  iosTools?: DeviceTool[];
  androidTools?: DeviceTool[];
  versions?: DeviceTool[];
  supported?: boolean;
} = {}): DevicesPlannedMock {
  const tools = new Map<"android" | "ios", DeviceTool[]>([
    ["ios", init.iosTools ?? iosTools(init.iosMissing === true)],
    ["android", init.androidTools ?? androidTools()],
  ]);
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const versions: DeviceTool[] = init.versions ?? [
    { id: "xcode", name: "Xcode command line tools", required: true, state: "ready", version: "16.2", latest: "16.2", note: "Installed 16.2" },
    { id: "ios-runtime", name: "iOS Simulator runtime", required: true, state: "update", version: "18.2", latest: "18.4", note: "Installed 18.2 · latest 18.4", steps: STEPS.runtime },
    { id: "agent-device", name: "agent-device", required: false, state: "ready", version: "0.9.3", latest: "0.9.3", note: "Installed 0.9.3 · latest 0.9.3" },
  ];
  let checkedAt: string | null = "14:02";
  let offline = init.offline === true;
  let access = init.access === true;
  let failInstall = false;
  const ready = (list: DeviceTool[]): DeviceTool[] => list.map((tool) => ({ ...tool }));
  const snapshot = (platform: "android" | "ios"): ToolSnapshot => ({
    platform,
    tools: ready(tools.get(platform) ?? []),
    access,
    checkedAt,
    offline,
    install: null,
  });
  return {
    checks: 0,
    accessWrites: [],
    restarts: [],
    installs: 0,
    failInstall: (on) => { failInstall = on; },
    setOffline: (next) => { offline = next; },
    setTools: (platform, list) => { tools.set(platform, list); },
    async status() {
      await wait();
      return { supported: init.supported !== false };
    },
    async tools(platform) {
      await wait();
      return snapshot(platform);
    },
    async check(platform) {
      this.checks += 1;
      await wait();
      checkedAt = offline ? checkedAt : "14:02";
      const list = tools.get(platform) ?? [];
      tools.set(platform, list.map((tool) => ({
        ...tool,
        state: offline ? "not_checked" : tool.version === null ? "missing" : tool.latest !== null && tool.latest !== tool.version ? "update" : "ready",
      })));
      return snapshot(platform);
    },
    async install(platform, onProgress: (progress: InstallProgress) => void) {
      this.installs += 1;
      const lines = ["$ orbyn install agent-device"];
      const emit = (progress: InstallProgress) => onProgress(progress);
      emit({ state: "installing", done: 0, total: 340, line: "Downloading agent-device-0.9.3.tgz", lines });
      await wait();
      emit({ state: "installing", done: 212, total: 340, line: "Downloading agent-device-0.9.3.tgz", lines });
      await wait();
      if (failInstall) {
        failInstall = false;
        lines.push("Error: Connection reset (ECONNRESET)");
        emit({ state: "failed", done: 212, total: 340, line: "Could not download. The connection dropped at 212 KB.", error: "Connection reset (ECONNRESET)", lines });
        throw new ApiFailure("tool_failed", "Could not download. The connection dropped at 212 KB.", 502);
      }
      lines.push("Installed agent-device 0.9.3");
      emit({ state: "done", done: 340, total: 340, line: "Installed agent-device 0.9.3", lines });
      for (const platform of ["ios", "android"] as const) {
        tools.set(platform, (tools.get(platform) ?? []).map((tool) => tool.id === "agent-device" ? { ...tool, state: "ready", version: "0.9.3" } : tool));
      }
      return snapshot(platform);
    },
    async setAccess(on) {
      await wait();
      access = on;
      this.accessWrites.push(on);
      return access;
    },
    async restart(id) {
      await wait();
      this.restarts.push(id);
      return true;
    },
    async versions(): Promise<ToolVersions> {
      await wait();
      return {
        tools: versions.map((tool) => ({ ...tool, state: offline ? "not_checked" : tool.state, note: offline && tool.version ? `Installed ${tool.version}` : tool.note })),
        checkedAt,
        offline,
      };
    },
  };
}
