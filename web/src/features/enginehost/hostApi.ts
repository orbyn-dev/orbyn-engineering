import { useEffect, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import type { EngineHostApi, HostCrash, HostRow, KeyStorage } from "./contract";

/**
 * The real client for the engine host routes. Those routes are not merged, so this one makes no request at all:
 * every read says the service reports nothing, and the screens draw nothing. Swap it for `request(...)` calls the
 * day the generated types and routes land; the screens do not change.
 */
const unreported: EngineHostApi = {
  hosts: async () => null,
  crashes: async () => [],
  storage: async () => null,
  retry: async () => { throw new Error("The engine host is not reported by this service."); },
  stop: async () => { throw new Error("The engine host is not reported by this service."); },
  decline: async () => { throw new Error("API key storage is not offered by this service."); },
  saveKey: async () => { throw new Error("API key storage is not offered by this service."); },
  removeKey: async () => { throw new Error("API key storage is not offered by this service."); },
};

let current: EngineHostApi = unreported;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/engine-host` installs the mock so the screens can be seen before the routes exist. In a
// production build the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/engine-host")) {
  void import("../../../mock/engineHost").then((module) => setEngineHostApi(module.createEngineHostMock({ delayMs: 400 })));
}

export const engineHostApi = (): EngineHostApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the unreported client back. */
export function setEngineHostApi(next: EngineHostApi | null): void {
  current = next ?? unreported;
  version += 1;
  for (const listener of listeners) listener();
}

function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

const TRANSITIONAL = new Set(["starting", "crashed", "restarting"]);
const POLL_MS = 1500;

/** The hosts the service reports, read again every 1.5 s while one is starting or waiting to restart. `null` when none are reported. */
export function useEngineHosts(): { hosts: HostRow[] | null; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`engine-hosts:${seen}`, (signal) => engineHostApi().hosts(signal));
  const hosts = read.status === "ready" ? read.data : null;
  const moving = hosts?.some((row) => TRANSITIONAL.has(row.state)) ?? false;
  const { reload, refreshQuietly } = read;
  useEffect(() => {
    if (!moving) return;
    const timer = setInterval(() => void refreshQuietly(), POLL_MS);
    return () => clearInterval(timer);
  }, [moving, refreshQuietly]);
  return { hosts, reload };
}

export function useEngineHostCrashes(): HostCrash[] {
  const seen = useApiVersion();
  const read = useAsync(`engine-host-crashes:${seen}`, (signal) => engineHostApi().crashes(signal));
  return read.status === "ready" ? read.data : [];
}

export function useKeyStorage(): { rows: KeyStorage[] | null; reload(): void; set(rows: KeyStorage[] | null): void } {
  const seen = useApiVersion();
  const read = useAsync(`key-storage:${seen}`, (signal) => engineHostApi().storage(signal));
  return { rows: read.status === "ready" ? read.data : null, reload: read.reload, set: read.setData };
}
