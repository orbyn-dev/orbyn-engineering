/**
 * MOCK of the engine host routes, which are not merged on the service yet. It is never called against the real
 * service: the app's own client reports "nothing" until the routes exist, and only a test or the dev-only preview
 * (`#/preview/engine-host`) installs this through `setEngineHostApi`. Fixtures follow the Engineer frames
 * `enginehost-*`; none holds a credential: a key sent to `saveKey` is dropped on arrival, and only the word `save:<id>` is recorded.
 */
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type EngineHostApi, type HostCrash, type HostRow, type KeyStorage, type StorageChoice } from "../src/features/enginehost/index.ts";
import { ApiFailure } from "../src/api/client.ts";

const at = (hours: number, minutes: number) => { const when = new Date(); when.setHours(hours, minutes, 0, 0); return when.toISOString(); };

/** One row per state, as the list frame shows them. */
export function hostFixtures(): HostRow[] {
  return [
    { id: "claude", name: "Claude Code · Personal", state: "running", since: at(9, 12), host_version: "0.1.0", protocol: 1,
      sdk: { name: "@anthropic-ai/claude-agent-sdk", version: "0.3.321" }, capabilities: ["steer", "fork", "background", "rollback", "models", "usage"], can_retry: false },
    { id: "claude:work", name: "Claude Code · Work", state: "stopped", stopped_at: new Date(Date.now() - 20 * 60_000).toISOString(), can_retry: false },
    { id: "cursor", name: "Cursor", state: "running", pooled: true, since: at(9, 30), host_version: "0.1.0", protocol: 1, can_retry: false },
    { id: "opencode", name: "OpenCode", state: "starting", can_retry: false },
    { id: "muse", name: "Muse", state: "not_started", can_retry: false },
    { id: "codex", name: "Codex", state: "restarting", stopped_at: at(9, 41), failed_threads: 2,
      restart: { attempt: 3, limit: 5, next_try_seconds: 4 },
      last_error: { kind: "host", message: "The engine host closed its output", causes: [{ name: "Error", message: "write EPIPE", code: "EPIPE" }] }, can_retry: false },
    { id: "acp", name: "ACP agents", state: "cant_start", stops_in_window: 5,
      last_error: { kind: "host", message: "the host did not say hello within 10 seconds", causes: [] }, can_retry: true },
    { id: "pi", name: "Pi", state: "version_mismatch", host_version: "0.2.0", protocol: 2, app_protocol: 1, can_retry: false },
  ];
}

export function crashFixtures(): HostCrash[] {
  return [{ id: "crash-1", at: at(9, 41), message: "The Codex engine host stopped: the engine host closed its output.", kind: "host",
    restart: { attempt: 3, limit: 5 }, failed_threads: 2, causes: [{ name: "Error", message: "write EPIPE", code: "EPIPE" }],
    stack: "at HostProcess.onExit (host.mjs:212)\nat ChildProcess.emit (node:events:518)\nat Process.onexit (node:child_process:293)" }];
}

export function storageFixtures(): KeyStorage[] {
  return [
    { engine_id: "cursor", name: "Cursor", choice: "saved", saved_at: new Date(2026, 10, 21, 9, 0).toISOString() },
    { engine_id: "codex", name: "Codex", choice: "declined" },
    { engine_id: "antigravity", name: "Antigravity", choice: "not_asked" },
  ];
}

export interface EngineHostMock extends EngineHostApi {
  /** Every write the screens made, in order: `retry:<id>`, `stop:<id>`, `save:<id>`, `decline:<id>`, `remove:<id>`, `keychain-retry:<id>`. */
  writes: string[];
  /** Make the next `saveKey` fail the way a locked or denied keychain does. */
  failNext(code: typeof KEYCHAIN_LOCKED | typeof KEYCHAIN_DENIED | null): void;
  /** Make the next `removeKey` fail. */
  failRemoveNext(on: boolean): void;
}

export function createEngineHostMock(init: { hosts?: HostRow[] | null; crashes?: HostCrash[]; storage?: KeyStorage[] | null; delayMs?: number; stopped?: number } = {}): EngineHostMock {
  let hosts = init.hosts === undefined ? hostFixtures() : init.hosts;
  let storage = init.storage === undefined ? storageFixtures() : init.storage;
  const writes: string[] = [];
  let fail: string | null = null;
  let failRemove = false;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const find = (id: string) => storage?.find((row) => row.engine_id === id);
  const put = (id: string, choice: StorageChoice, extra: Partial<KeyStorage> = {}) => {
    const row = find(id);
    if (!row) throw new ApiFailure("not_found", "No such engine.", 404);
    const next = { ...row, choice, ...extra };
    storage = (storage ?? []).map((entry) => (entry.engine_id === id ? next : entry));
    return next;
  };
  return {
    writes,
    failNext: (code) => { fail = code; },
    failRemoveNext: (on) => { failRemove = on; },
    hosts: async () => hosts,
    crashes: async () => init.crashes ?? crashFixtures(),
    storage: async () => storage,
    retry: async (id) => { writes.push(`retry:${id}`); await wait(); hosts = (hosts ?? []).map((row) => (row.id === id ? { ...row, state: "starting", can_retry: false, last_error: null } : row)); },
    stop: async (id) => { writes.push(`stop:${id}`); await wait(); hosts = (hosts ?? []).map((row) => (row.id === id ? { ...row, state: "stopped", stopped_at: new Date().toISOString(), restart: null } : row)); },
    decline: async (id) => { writes.push(`decline:${id}`); await wait(); return put(id, "declined", { saved_at: null }); },
    // The key is dropped here on purpose: a mock that kept it would be a second place a secret lives.
    saveKey: async (id, _key) => {
      writes.push(`save:${id}`);
      await wait();
      if (fail) { const code = fail; fail = null; throw new ApiFailure(code, code === KEYCHAIN_LOCKED ? "The keychain is locked." : "The keychain refused.", 409); }
      return put(id, "saved", { saved_at: new Date().toISOString() });
    },
    removeOptIn: async (id) => { writes.push(`optout:${id}`); await wait(); return put(id, "not_asked", { saved_at: null }); },
    removeKey: async (id) => {
      writes.push(`remove:${id}`);
      await wait();
      if (failRemove) { failRemove = false; throw new ApiFailure("keychain_remove_failed", "The keychain could not remove it.", 409); }
      return put(id, "declined", { saved_at: null, stopped: init.stopped ?? 0 });
    },
    retryKeychain: async (id) => { writes.push(`keychain-retry:${id}`); await wait(); const row = find(id); if (!row) throw new ApiFailure("not_found", "No such engine.", 404); return row; },
  };
}
