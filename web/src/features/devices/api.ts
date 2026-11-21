import { useEffect, useState } from "react";
import { ApiFailure, request } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import type { ActionResult, DeviceAction, DeviceDetail, DeviceList, DevicesApi } from "./contract";

const q = encodeURIComponent;
const isMissing = (error: unknown) => error instanceof ApiFailure && error.status === 404;

/**
 * The real client for the routes of docs/devices.md (provisional; not run against a real service). `GET /api/devices` answering `404`
 * is "this service has no devices" and is answered as `null` with no error, so nothing draws. Every write is Person-class
 * (`personKey.ts`): `request` adds the key. The device id in a path is encoded, and only ever one the list gave.
 */
const http: DevicesApi = {
  async list(signal) {
    try {
      const list = await request<DeviceList>("GET", "/api/devices", undefined, signal);
      // An answer that is not a device list (some other service behind the same route) is "no devices feature", not a crash.
      if (typeof list !== "object" || list === null || !Array.isArray(list.devices)) return null;
      return { devices: list.devices, platforms: Array.isArray(list.platforms) ? list.platforms : [] };
    } catch (error) {
      if (isMissing(error)) return null;
      throw error;
    }
  },
  detail: (id, signal) => request<DeviceDetail>("GET", `/api/devices/${q(id)}`, undefined, signal),
  act: (id, action: DeviceAction) => request<ActionResult>("POST", `/api/devices/${q(id)}/actions`, action),
};

let current: DevicesApi = http;
let version = 0;
/** Whether the service has the feature: read once per client, so the panel strip asks the service one time. */
let probe: Promise<boolean> | null = null;
const listeners = new Set<() => void>();

// Dev only: `#/preview/devices` installs the mock so the panel can be seen before the routes exist. In a production build the
// flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/devices")) {
  void import("../../../mock/devices").then((module) => setDevicesApi(module.createDevicesMock({ delayMs: 300 })));
}

export const devicesApi = (): DevicesApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setDevicesApi(next: DevicesApi | null): void {
  current = next ?? http;
  probe = null;
  version += 1;
  for (const listener of listeners) listener();
}

/** Changes when the client is swapped, so a screen reads again from the new one. */
export function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** `true` once the service answered the list; `false` while asking, on `404`, and on any failure (the feature stays hidden, no error). */
export function useDevicesOffered(): boolean {
  const seen = useApiVersion();
  const [offered, setOffered] = useState(false);
  useEffect(() => {
    let live = true;
    probe ??= devicesApi().list().then((list) => list !== null, () => false);
    void probe.then((answer) => { if (live) setOffered(answer); });
    return () => { live = false; };
  }, [seen]);
  return offered;
}

/** The device list. `list` is `null` while loading and when the service reports no devices feature. */
export function useDevices(): { list: DeviceList | null; error: ApiFailure | null; settled: boolean; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`devices:${seen}`, (signal) => devicesApi().list(signal));
  return {
    list: read.status === "ready" ? read.data : null,
    error: read.status === "error" ? read.error : null,
    settled: read.status !== "loading",
    reload: read.reload,
  };
}
