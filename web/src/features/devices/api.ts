import { useEffect, useState } from "react";
import { ApiFailure, request } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import type { ActionResult, DeviceAction, DeviceDetail, DeviceList, DevicesApi, PlatformStatus } from "./contract";
import { usePlannedSupport } from "./planned";

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
/** What the one strip probe found: read once per client, so the panel strip asks the service one time. */
type Probe = { kind: "list"; list: DeviceList } | { kind: "missing" } | { kind: "failed" };
let probe: Promise<Probe> | null = null;
const listeners = new Set<() => void>();

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

/** How the "+" launcher and the tab strip should treat Devices. Phone handling is the caller's. */
export interface DevicesEntry {
  /** `loading` while the one probe runs; `missing` is a `404` (no devices route); `failed` is any other failure. */
  state: "loading" | "ready" | "missing" | "failed";
  /** Draw the row at all. A service without the route draws it disabled with the reason; a failure draws nothing. */
  shown: boolean;
  /** Open the tab. False when no platform is available and the Planned setup surface does not answer. */
  enabled: boolean;
  /** Why a shown row is disabled, from the service's platform sentence when there is one. */
  reason: string;
  anyAvailable: boolean;
  platforms: PlatformStatus[];
}

const UNAVAILABLE = "Devices are not available on this service.";
const NO_TOOLS = "No device tools were found on this computer.";

/**
 * The strip and launcher's one answer. The service answered with a list: the tab opens, and when no platform is
 * available it opens the setup hub instead. A `404` is drawn disabled with the reason (the contract's "no feature":
 * no tab opens and no error is shown). Any other failure leaves Devices out as before. The Planned setup client
 * decides whether a computer with no tools can still reach the setup hub; in production it answers nothing, so
 * the entry is disabled with the service's sentence.
 */
export function useDevicesEntry(): DevicesEntry {
  const seen = useApiVersion();
  const setupSupported = usePlannedSupport();
  const [found, setFound] = useState<Probe | null>(null);
  useEffect(() => {
    let live = true;
    probe ??= devicesApi().list().then(
      (list): Probe => (list === null ? { kind: "missing" } : { kind: "list", list }),
      (): Probe => ({ kind: "failed" }),
    );
    void probe.then((answer) => { if (live) setFound(answer); });
    return () => { live = false; };
  }, [seen]);

  if (found === null || (found.kind === "list" && setupSupported === null)) {
    return { state: "loading", shown: false, enabled: false, reason: "", anyAvailable: false, platforms: [] };
  }
  if (found.kind === "failed") {
    return { state: "failed", shown: false, enabled: false, reason: "", anyAvailable: false, platforms: [] };
  }
  if (found.kind === "missing") {
    return { state: "missing", shown: true, enabled: false, reason: UNAVAILABLE, anyAvailable: false, platforms: [] };
  }
  const platforms = found.list.platforms;
  // A list without platform status is older than the contract's platforms field: the feature stays offered.
  const anyAvailable = platforms.length === 0 || platforms.some((one) => one.available);
  const enabled = anyAvailable || setupSupported === true;
  const first = platforms.find((one) => !one.available)?.message || NO_TOOLS;
  return {
    state: "ready",
    shown: true,
    enabled,
    reason: enabled ? "" : first,
    anyAvailable,
    platforms,
  };
}

/** `true` once the service answered the list and the Devices tab can open; false while asking, on `404`, and on failure. */
export function useDevicesOffered(): boolean {
  const entry = useDevicesEntry();
  return entry.state === "ready" && entry.enabled;
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
