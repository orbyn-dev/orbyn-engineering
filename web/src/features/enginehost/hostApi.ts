import { useEffect, useRef, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import { createCredentialsApi } from "./credentialsApi";
import type { EngineHostApi, HostCrash, HostRow, KeyStorage } from "./contract";

/**
 * The real client for the engine host and API key storage. The host routes are not merged, so `hosts`, `crashes`, `retry` and `stop`
 * make no request at all: every read says the service reports nothing, and those screens draw nothing. Swap them for `request(...)`
 * calls when the routes and generated types land. API key storage is real: `credentialsApi.ts` calls
 * `/api/settings/engine-credentials/{engine}` (not yet run against a real service).
 */
const credentials = createCredentialsApi();
const real: EngineHostApi = {
  hosts: async () => null,
  crashes: async () => [],
  retry: async () => { throw new Error("The engine host is not reported by this service."); },
  stop: async () => { throw new Error("The engine host is not reported by this service."); },
  storage: (signal) => credentials.storage(signal),
  decline: (id) => credentials.decline(id),
  saveKey: (id, key) => credentials.saveKey(id, key),
  removeKey: (id) => credentials.removeKey(id),
  retryKeychain: (id) => credentials.retryKeychain(id),
};

let current: EngineHostApi = real;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/engine-host` installs the mock so the screens can be seen before the routes exist. In a
// production build the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/engine-host")) {
  void import("../../../mock/engineHost").then((module) => setEngineHostApi(module.createEngineHostMock({ delayMs: 400 })));
}

export const engineHostApi = (): EngineHostApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setEngineHostApi(next: EngineHostApi | null): void {
  current = next ?? real;
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

export function useKeyStorage(): { rows: KeyStorage[] | null; reload(): void; replace(next: KeyStorage): void } {
  const seen = useApiVersion();
  const read = useAsync(`key-storage:${seen}`, (signal) => engineHostApi().storage(signal));
  const rows = read.status === "ready" ? read.data : null;
  // Every answer is applied to the rows as they are when it lands, not as they were at the click: two writes that settle before a
  // render each keep their own row, and a slower answer for one engine cannot put an older copy of another back.
  const latest = useRef<KeyStorage[] | null>(rows);
  latest.current = rows;
  const { setData } = read;
  const replace = (next: KeyStorage) => {
    const updated = (latest.current ?? []).map((row) => (row.engine_id === next.engine_id ? next : row));
    latest.current = updated;
    setData(updated);
  };
  return { rows, reload: read.reload, replace };
}
