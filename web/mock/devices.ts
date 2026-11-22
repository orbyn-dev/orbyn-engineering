/**
 * MOCK of the device routes of docs/devices.md (provisional), for tests and the dev-only preview (`#/preview/devices`), which install
 * it through `setDevicesApi`. The app's own client is the real HTTP one in src/features/devices/api.ts. Fixtures follow the
 * Device frames: the list, tabs, states and setup screens. It refuses what the doc says the service refuses: `device_not_booted`,
 * `not_foldable`, `action_unsupported` (iOS rotate and buttons) and `device_busy`. The posture it answers for a hinge angle is the
 * mock's own guess (the doc leaves open what Android reports at 130); the panel must render whatever comes back.
 *
 * `createDevicesPlannedMock` covers the pieces docs/devices.md does not define (tools, their versions, the one Orbyn install, the
 * agent-access toggle, the H26 device-tools drawer, the H27 permission flow, the live screen stream and its pointer/typing input,
 * and the driving agent's hold). The production client for those answers nothing; only this mock makes the setup, tools, agent and
 * live frames interactive.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { ActionResult, ActionType, Device, DeviceAction, DeviceDetail, DeviceList, DeviceState, DevicesApi, Fold, PlatformStatus } from "../src/features/devices/contract.ts";
import type { AgentAccessState, AgentDeviceGrant, AgentDeviceRequest, DeviceAgentState, DeviceLiveFrame, DevicePoint, DevicePointerInput, DeviceToolChange, DeviceToolsState, DeviceTool, DevicesLivePlannedApi, DevicesPlannedApi, InstallProgress, PushOutcome, ToolSnapshot, ToolVersions } from "../src/features/devices/planned.ts";


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
    { id: "ios-runtime", name: "iOS Simulator runtime", required: true, state: missing ? "missing" : "ready", version: missing ? null : "18.2", latest: "18.2", note: missing ? "Needs Xcode first" : "iOS 18.2 · 7.9 GB", steps: STEPS.runtime },
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

const TEXT_STEPS = ["Small", "Smaller", "Default", "Large", "Larger", "Extra large", "Accessibility 1", "Accessibility 2", "Accessibility 3", "Accessibility 4", "Accessibility 5", "Accessibility 6"];

/** The H26 drawer's default state. Nothing here is a production route. */
function defaultToolsState(): DeviceToolsState {
  return {
    appearance: { mode: "light", increaseContrast: false, reduceMotion: false },
    text: { step: 4, total: TEXT_STEPS.length, name: TEXT_STEPS[3]!, bold: false },
    accessibility: {
      showElements: true,
      elements: [
        { id: "sign-in", label: "Sign in", role: "Button", detail: "Label “Sign in”, enabled, 192 × 32" },
        { id: "email", label: "Email", role: "Text field", detail: "Label “Email”, empty" },
      ],
    },
    location: {
      place: "Berlin, Germany",
      coordinates: "52.5200, 13.4050",
      places: ["None", "Berlin, Germany", "London, UK", "San Francisco, CA", "Tokyo, Japan"],
    },
    permissions: [
      { id: "camera", name: "Camera", app: "Acme", askedOnce: true, decision: "ask" },
      { id: "notifications", name: "Notifications", app: "Acme", askedOnce: false, decision: "allow" },
      { id: "location", name: "Location", app: "Acme", askedOnce: false, decision: "deny" },
    ],
    push: {
      app: "Acme",
      apps: ["Acme", "Orbyn Preview"],
      payload: '{ "aps": { "alert": { "title": "Order shipped", "body": "Arrives Friday" } } }',
    },
  };
}

export interface DevicesPlannedMock extends DevicesPlannedApi, DevicesLivePlannedApi {
  /** How many times a version check ran. */
  checks: number;
  /** Every agent-access write, in order. */
  accessWrites: boolean[];
  /** Every restart request, by device id. */
  restarts: string[];
  /** How many agent-device installs started. */
  installs: number;
  /** Every device-tools change, as `<device id>:<type>:<value>`. */
  toolWrites: string[];
  /** Every push send, by device id. */
  pushWrites: string[];
  /** The current tools state for one device, or null when none was read yet. */
  toolState(id: string): DeviceToolsState | null;
  /** Make the next push answer the platform's failure line. */
  failNextPush(on: boolean): void;
  /** How many live screen streams started. */
  liveStarts: number;
  /** How many live screen frames were sent. */
  framesSent: number;
  /** Every pointer sent, in order. */
  pointers: DevicePointerInput[];
  /** Every typed string, in order; `\n` is Return. */
  typed: string[];
  /** Every agent control change, in order. */
  agentChanges: ("take_over" | "hand_back")[];
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
  tools?: Partial<DeviceToolsState>;
  agentRequest?: AgentDeviceRequest | null;
  agentGrant?: AgentDeviceGrant | null;
  agentReason?: string;
  /** The Planned live screen. Omitted or `false` means the service has no live stream (`null`, Planned). */
  live?: {
    screen?: string;
    cursor?: DevicePoint;
    agent?: "off" | "driving" | "paused";
    agentName?: string;
    delayMs?: number;
  } | false;
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
  let failPush = false;
  const toolsById = new Map<string, DeviceToolsState>();
  let agent: AgentAccessState = {
    enabled: access,
    request: init.agentRequest ?? null,
    grant: init.agentGrant ?? null,
    ...(init.agentReason ? { reason: init.agentReason } : {}),
  };
  const toolState = (id: string): DeviceToolsState => {
    const held = toolsById.get(id);
    if (held) return held;
    const next: DeviceToolsState = { ...defaultToolsState(), ...init.tools };
    toolsById.set(id, next);
    return next;
  };
  const answer = (id: string, change: DeviceToolChange): DeviceToolsState => {
    const state = toolState(id);
    const next: DeviceToolsState = {
      appearance: { ...state.appearance },
      text: { ...state.text },
      accessibility: { ...state.accessibility, elements: state.accessibility.elements },
      location: { ...state.location },
      permissions: state.permissions.map((one) => ({ ...one })),
      push: { ...state.push },
    };
    switch (change.type) {
      case "appearance": next.appearance.mode = change.mode; break;
      case "increase_contrast": next.appearance.increaseContrast = change.on; break;
      case "reduce_motion": next.appearance.reduceMotion = change.on; break;
      case "text_size": {
        const step = Math.min(next.text.total, Math.max(1, Math.round(change.step)));
        next.text = { ...next.text, step, name: TEXT_STEPS[step - 1] ?? next.text.name };
        break;
      }
      case "bold": next.text.bold = change.on; break;
      case "show_elements": next.accessibility.showElements = change.on; break;
      case "location":
        next.location.place = change.place;
        if (change.place === null) next.location.coordinates = "";
        else if (change.coordinates !== undefined) next.location.coordinates = change.coordinates;
        else next.location.coordinates = state.location.coordinates;
        break;
      case "permission":
        next.permissions = next.permissions.map((one) => one.id === change.id ? { ...one, decision: change.decision } : one);
        break;
      case "push_app": next.push.app = change.app; break;
    }
    toolsById.set(id, next);
    return next;
  };
  const toolWrites: string[] = [];
  const pushWrites: string[] = [];
  const live = init.live === false || init.live === undefined ? null : init.live;
  let hold: DeviceAgentState = {
    status: live?.agent ?? "off",
    name: live && (live.agent === "driving" || live.agent === "paused") ? live.agentName ?? "Claude Code" : null,
  };
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
    toolWrites,
    pushWrites,
    toolState: (id) => {
      const held = toolsById.get(id);
      return held ? { ...held } : null;
    },
    failNextPush: (on) => { failPush = on; },
    liveStarts: 0,
    framesSent: 0,
    pointers: [],
    typed: [],
    agentChanges: [],
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
      agent = { ...agent, enabled: on, ...(on ? {} : { request: null, grant: null }) };
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
    async deviceTools(id) {
      await wait();
      return toolState(id);
    },
    async setDeviceTool(id, change) {
      await wait();
      const suffix = change.type === "location"
        ? change.place === null ? "none" : `${change.coordinates ?? ""}`
        : change.type === "permission" ? `${change.id}:${change.decision}`
        : change.type === "push_app" ? change.app
        : change.type === "appearance" ? change.mode
        : change.type === "text_size" ? `${change.step}`
        : change.type === "increase_contrast" || change.type === "reduce_motion" || change.type === "bold" || change.type === "show_elements" ? `${change.on}`
        : "";
      toolWrites.push(`${id}:${change.type}:${suffix}`);
      return answer(id, change);
    },
    async sendDevicePush(id, payload) {
      await wait();
      if (id.startsWith("android:")) return { ok: false, error: "Not available on Android" } satisfies PushOutcome;
      if (failPush) {
        failPush = false;
        return { ok: false, error: "The device did not accept the notification." } satisfies PushOutcome;
      }
      const state = toolState(id);
      toolsById.set(id, { ...state, push: { ...state.push, payload } });
      pushWrites.push(id);
      return { ok: true } satisfies PushOutcome;
    },
    async agentAccess() {
      await wait();
      return { ...agent };
    },
    async setAgentAccess(on) {
      await wait();
      access = on;
      agent = { ...agent, enabled: on, reason: undefined, ...(on ? {} : { request: null, grant: null }) };
      this.accessWrites.push(on);
      return { ...agent };
    },
    async answerAgentRequest(id, decision) {
      await wait();
      if (!agent.enabled) throw new ApiFailure("agent_access_off", "Agent access is off.", 403);
      const request = agent.request;
      if (!request || request.id !== id) return { ...agent };
      if (decision === "deny") agent = { ...agent, request: null, grant: null };
      else agent = {
        ...agent,
        request: null,
        grant: { agent: request.agent, device: request.device, scope: decision === "once" ? "once" : "thread", thread: request.thread, by: "person" },
      };
      return { ...agent };
    },
    async revokeAgentAccess() {
      await wait();
      agent = { ...agent, grant: null };
      return { ...agent };
    },
    async stream(_id, onFrame: (frame: DeviceLiveFrame) => void, signal?: AbortSignal) {
      if (!live) return null;
      this.liveStarts += 1;
      const frame: DeviceLiveFrame = {
        image: live.screen ?? PIXEL_PNG,
        ...(live.cursor ? { cursor: live.cursor } : {}),
      };
      const send = () => {
        if (signal?.aborted) return;
        this.framesSent += 1;
        onFrame(frame);
      };
      const delay = live.delayMs ?? init.delayMs ?? 0;
      if (delay > 0) setTimeout(send, delay);
      else send();
      return true;
    },
    async pointer(_id, input: DevicePointerInput) {
      if (!live) return null;
      this.pointers.push(input);
      // Touching the device takes control from the agent (device-agent-paused).
      if (hold.status === "driving") hold = { ...hold, status: "paused" };
      return true;
    },
    async typeText(_id, text: string) {
      if (!live) return null;
      this.typed.push(text);
      return true;
    },
    async agentState(_id, change?: "take_over" | "hand_back") {
      if (!live) return null;
      if (change === "take_over") {
        this.agentChanges.push("take_over");
        hold = { ...hold, status: "paused" };
      } else if (change === "hand_back") {
        this.agentChanges.push("hand_back");
        hold = { ...hold, status: "driving", name: hold.name ?? live.agentName ?? "Claude Code" };
      }
      return { ...hold };
    },
  };
}
