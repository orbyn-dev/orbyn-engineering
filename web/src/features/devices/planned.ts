/**
 * PROVISIONAL: the pieces the Devices frames need that `docs/devices.md` does not define (tool install, version
 * check, an agent-access toggle and restarting a device). There is NO production implementation: the default
 * client below answers `null` and calls nothing, so the UI draws the frame's Planned or unavailable state. The
 * dev-only mock (`web/mock/devices.ts`) installs a typed client for tests and `#/preview/devices`.
 */

import { useEffect, useState } from "react";
import type { Device, Platform } from "./contract";

/** A row's state in the setup hub. `not_checked` is an installed version whose update was not checked. */
export type ToolState = "ready" | "missing" | "update" | "not_checked" | "checking" | "failed";

/** The vendor's own instructions. Text is untrusted as everywhere: it is drawn capped, never as markup. */
export interface ToolStep {
  title: string;
  text: string;
  command?: string;
  url?: string;
  domain?: string;
}

export interface DeviceTool {
  /** A fixed id from this client; never used as a command or a selector. */
  id: string;
  name: string;
  /** Required tools block Continue; agent-device is optional. */
  required: boolean;
  state: ToolState;
  version: string | null;
  /** The latest tested version, when a check answered. */
  latest: string | null;
  /** A byte label the service sent, such as `212 of 340 KB`. */
  size?: string;
  note?: string;
  steps?: ToolStep;
}

/** What the one install Orbyn runs reports. The mock emits these; the production client never starts. */
export interface InstallProgress {
  state: "installing" | "failed" | "done";
  done: number;
  total: number;
  line: string;
  error?: string;
  lines: string[];
}

export interface ToolSnapshot {
  platform: Platform;
  tools: DeviceTool[];
  /** The agent-access setting (H27, Planned): off until the person turns it on. */
  access: boolean;
  /** A display string, when a check has run. */
  checkedAt: string | null;
  offline: boolean;
  install: InstallProgress | null;
}

export interface ToolVersions {
  tools: DeviceTool[];
  checkedAt: string | null;
  offline: boolean;
}

export interface DevicesPlannedApi extends DeviceToolsPlannedApi, AgentDevicePlannedApi {
  /** Whether the setup and tool checks exist at all. `null` is "Planned": no route answers. */
  status(signal?: AbortSignal): Promise<{ supported: boolean } | null>;
  tools(platform: Platform, signal?: AbortSignal): Promise<ToolSnapshot | null>;
  check(platform: Platform, signal?: AbortSignal): Promise<ToolSnapshot | null>;
  install(
    platform: Platform,
    onProgress: (progress: InstallProgress) => void,
    signal?: AbortSignal,
  ): Promise<ToolSnapshot | null>;
  setAccess(on: boolean): Promise<boolean | null>;
  /** Ask the platform tool to start a device the person did not shut down. `null` is Planned. */
  restart(id: string): Promise<boolean | null>;
  versions(signal?: AbortSignal): Promise<ToolVersions | null>;
}

/** No production implementation: every read answers `null` and no request leaves the app. */
const absent: DevicesPlannedApi = {
  status: async () => null,
  tools: async () => null,
  check: async () => null,
  install: async () => null,
  setAccess: async () => null,
  restart: async () => null,
  versions: async () => null,
  deviceTools: async () => null,
  setDeviceTool: async () => null,
  sendDevicePush: async () => null,
  agentAccess: async () => null,
  setAgentAccess: async () => null,
  answerAgentRequest: async () => null,
  revokeAgentAccess: async () => null,
};

let current: DevicesPlannedApi = absent;
let version = 0;
const listeners = new Set<() => void>();

export const devicesPlannedApi = (): DevicesPlannedApi => current;

/** Tests and the dev-only preview install a client here; `null` puts the absent production client back. */
export function setDevicesPlannedApi(next: DevicesPlannedApi | null): void {
  current = next ?? absent;
  version += 1;
  for (const listener of listeners) listener();
}

/** Changes when the planned client is swapped, so a screen reads again from the new one. */
export function usePlannedApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** Whether the planned surface answers. `null` while asking; `false` in production (the setup and checks are Planned). */
export function usePlannedSupport(): boolean | null {
  const seen = usePlannedApiVersion();
  const [supported, setSupported] = useState<boolean | null>(null);
  useEffect(() => {
    let live = true;
    void devicesPlannedApi().status().then(
      (answer) => { if (live) setSupported(answer?.supported === true); },
      () => { if (live) setSupported(false); },
    );
    return () => { live = false; };
  }, [seen]);
  return supported;
}

/** The restart action's own words, shared by the frozen and crashed overlays. */
export const restartLabel = (state: Device["state"]): string => (state === "crashed" ? "Start it again" : "Restart device");

/* ------------------------------------------------------------------------------------------------------------------ */
/* H26 device tools and H27 agent permission.                                                                          */
/* No service route defines any of this yet: production answers `null` and sends nothing, and every screen draws its    */
/* Planned state. The dev-only mock fills these in for tests and `#/preview/devices`. This block is appended so another  */
/* run's live-screen members can merge beside it without touching these types or methods.                              */
/* ------------------------------------------------------------------------------------------------------------------ */

export type AppearanceMode = "light" | "dark";
export type PermissionDecision = "ask" | "allow" | "deny";
export type AgentRequestDecision = "once" | "thread" | "deny";

export interface ToolsAppearance {
  mode: AppearanceMode;
  increaseContrast: boolean;
  reduceMotion: boolean;
}

export interface ToolsText {
  /** The step the device is on, 1 based, and how many steps the platform has. */
  step: number;
  total: number;
  /** The platform's own name for the step, such as `Large`. */
  name: string;
  bold: boolean;
}

export interface ToolsAccessibilityElement {
  id: string;
  label: string;
  role: string;
  /** The element's text, as the platform's tree reports it. Untrusted: drawn as text, capped. */
  detail: string;
}

export interface ToolsAccessibility {
  showElements: boolean;
  elements: ToolsAccessibilityElement[];
}

export interface ToolsLocation {
  /** `null` is the frame's None: the device gets no simulated place. */
  place: string | null;
  coordinates: string;
  places: string[];
}

export interface ToolsPermission {
  id: string;
  name: string;
  app: string;
  askedOnce: boolean;
  decision: PermissionDecision;
}

export interface ToolsPush {
  app: string;
  apps: string[];
  /** The payload kept per device. */
  payload: string;
}

/** The drawers' one read. Every field is the device's own setting, not Orbyn's. */
export interface DeviceToolsState {
  appearance: ToolsAppearance;
  text: ToolsText;
  accessibility: ToolsAccessibility;
  location: ToolsLocation;
  permissions: ToolsPermission[];
  push: ToolsPush;
}

/** One typed change. The service route, when it exists, reads back the state after the write. */
export type DeviceToolChange =
  | { type: "appearance"; mode: AppearanceMode }
  | { type: "increase_contrast"; on: boolean }
  | { type: "reduce_motion"; on: boolean }
  | { type: "text_size"; step: number }
  | { type: "bold"; on: boolean }
  | { type: "show_elements"; on: boolean }
  | { type: "location"; place: string | null; coordinates?: string }
  | { type: "permission"; id: string; decision: PermissionDecision }
  | { type: "push_app"; app: string };

export interface PushOutcome {
  ok: boolean;
  /** The platform's own failure line, shown under the editor. */
  error?: string;
}

export interface AgentDeviceRequest {
  id: string;
  agent: string;
  device: string;
  thread: string | null;
}

export interface AgentDeviceGrant {
  agent: string;
  device: string;
  scope: "once" | "thread";
  thread: string | null;
  /** `lead` is the Autopilot Full answer (H27); a person's answer leaves this out. */
  by?: "person" | "lead";
  mode?: string;
}

/** H27's one state. Off is the default, and nothing here ever turns it on but a person's choice. */
export interface AgentAccessState {
  enabled: boolean;
  request: AgentDeviceRequest | null;
  grant: AgentDeviceGrant | null;
  /** A plain sentence the service gives when it refuses to turn access on (H42). */
  reason?: string;
}

/** The H26 methods. `null` is Planned: no route answers. */
export interface DeviceToolsPlannedApi {
  deviceTools(id: string, signal?: AbortSignal): Promise<DeviceToolsState | null>;
  setDeviceTool(id: string, change: DeviceToolChange): Promise<DeviceToolsState | null>;
  sendDevicePush(id: string, payload: string): Promise<PushOutcome | null>;
}

/** The H27 methods. `null` is Planned: the agent-device CLI and its permission flow are not integrated. */
export interface AgentDevicePlannedApi {
  agentAccess(signal?: AbortSignal): Promise<AgentAccessState | null>;
  /** A person's choice only. The UI never calls this to turn access on by itself. */
  setAgentAccess(on: boolean): Promise<AgentAccessState | null>;
  answerAgentRequest(id: string, decision: AgentRequestDecision, reply?: string): Promise<AgentAccessState | null>;
  /** Drops the current grant; the device itself keeps running for the person. */
  revokeAgentAccess(): Promise<AgentAccessState | null>;
}
/* ---------------------------------------------------------------------------------------------------------------- */
/* Phase 2 (H22, H23, H27): the live screen stream, its pointer and text input, and the agent that may be driving.  */
/* docs/devices.md defines none of this yet. The members live on their own typed surface below so this block stays  */
/* additive: the production default answers `null` and sends nothing, and the dev-only mock supplies the frames.     */
/* ---------------------------------------------------------------------------------------------------------------- */

/** A point as fractions of the device screen (0 to 1), never a device pixel or a device-chosen selector. */
export interface DevicePoint {
  x: number;
  y: number;
}

/** Touch, swipe and scroll as normalized screen coordinates. */
export type DevicePointerInput =
  | { type: "tap"; x: number; y: number }
  | { type: "swipe"; from: DevicePoint; to: DevicePoint; durationMs: number }
  | { type: "scroll"; x: number; y: number; deltaY: number };

/** One live screen frame: a PNG as base64, plus the agent's cursor when it is driving. */
export interface DeviceLiveFrame {
  image: string;
  cursor?: DevicePoint;
}

/** Who holds the device: the person (off), the agent, or the agent paused by the person. */
export interface DeviceAgentState {
  status: "off" | "driving" | "paused";
  /** The agent's own name, untrusted text: it is drawn capped and escaped, never as markup. */
  name: string | null;
}

/** The Planned live members. No production implementation: every call answers `null` and sends nothing. */
export interface DevicesLivePlannedApi {
  /** Start the stream. `null` is Planned; `true` means frames may arrive until `signal` aborts. */
  stream(id: string, onFrame: (frame: DeviceLiveFrame) => void, signal?: AbortSignal): Promise<boolean | null>;
  /** Send one touch, swipe or scroll. `null` is Planned. */
  pointer(id: string, input: DevicePointerInput): Promise<boolean | null>;
  /** Type text into the focused device field; `\n` is Return. `null` is Planned. */
  typeText(id: string, text: string): Promise<boolean | null>;
  /** Read the agent's hold on the device, or ask for it (`take_over`) or release it (`hand_back`). `null` is Planned. */
  agentState(id: string, change?: "take_over" | "hand_back", signal?: AbortSignal): Promise<DeviceAgentState | null>;
}

const absentLive: DevicesLivePlannedApi = {
  stream: async () => null,
  pointer: async () => null,
  typeText: async () => null,
  agentState: async () => null,
};

const liveMembers = (api: DevicesPlannedApi): DevicesLivePlannedApi | null => {
  const candidate = api as Partial<DevicesLivePlannedApi>;
  return typeof candidate.stream === "function"
    && typeof candidate.pointer === "function"
    && typeof candidate.typeText === "function"
    && typeof candidate.agentState === "function"
    ? candidate as DevicesLivePlannedApi
    : null;
};

/** The live surface of the installed Planned client, or the production default that sends nothing. */
export const devicesLivePlannedApi = (): DevicesLivePlannedApi => liveMembers(current) ?? absentLive;
