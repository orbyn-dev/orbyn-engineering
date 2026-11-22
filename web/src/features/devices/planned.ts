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

export interface DevicesPlannedApi {
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
