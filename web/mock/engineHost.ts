/**
 * MOCK of the engine host routes, which are not merged on the service yet. It is never called against the real
 * service: the app's own client reports "nothing" until the routes exist, and only a test or the dev-only preview
 * (`#/preview/engine-host`) installs this through `setEngineHostApi`. Fixtures follow the Engineer frames
 * `enginehost-*`; none holds a credential, because the contract has no field for one.
 */
import {
  KEYCHAIN_DENIED, KEYCHAIN_LOCKED,
  type EngineHostApi, type HostCrash, type HostRow, type SignInStorage, type StorageChoice,
} from "../src/features/settings/engineHost/contract.ts";
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

export function storageFixtures(): SignInStorage[] {
  return [
    { engine_id: "cursor", name: "Cursor", choice: "allowed", allowed_at: new Date(2026, 10, 21, 9, 0).toISOString(), running_threads: 2 },
    { engine_id: "codex", name: "Codex", choice: "declined", running_threads: 0 },
    { engine_id: "antigravity", name: "Antigravity", choice: "not_asked", running_threads: 0 },
  ];
}

export interface EngineHostMock extends EngineHostApi {
  /** Every write the screens made, in order: `retry:<id>`, `allow:<id>`, `decline:<id>`, `revoke:<id>`. */
  writes: string[];
  /** Make the next `choose` fail the way a locked or denied keychain does. */
  failNext(code: typeof KEYCHAIN_LOCKED | typeof KEYCHAIN_DENIED | null): void;
}

export function createEngineHostMock(init: { hosts?: HostRow[] | null; crashes?: HostCrash[]; storage?: SignInStorage[] | null; delayMs?: number } = {}): EngineHostMock {
  let hosts = init.hosts === undefined ? hostFixtures() : init.hosts;
  let storage = init.storage === undefined ? storageFixtures() : init.storage;
  const writes: string[] = [];
  let fail: string | null = null;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const find = (id: string) => storage?.find((row) => row.engine_id === id);
  const put = (id: string, choice: StorageChoice, extra: Partial<SignInStorage> = {}) => {
    const row = find(id);
    if (!row) throw new ApiFailure("not_found", "No such engine.", 404);
    const next = { ...row, choice, ...extra };
    storage = (storage ?? []).map((entry) => (entry.engine_id === id ? next : entry));
    return next;
  };
  return {
    writes,
    failNext: (code) => { fail = code; },
    hosts: async () => hosts,
    crashes: async () => init.crashes ?? crashFixtures(),
    storage: async () => storage,
    retry: async (id) => { writes.push(`retry:${id}`); await wait(); hosts = (hosts ?? []).map((row) => (row.id === id ? { ...row, state: "starting", can_retry: false, last_error: null } : row)); },
    choose: async (id, choice) => {
      writes.push(`${choice}:${id}`);
      await wait();
      if (fail) { const code = fail; fail = null; throw new ApiFailure(code, code === KEYCHAIN_LOCKED ? "The keychain is locked." : "The keychain refused.", 409); }
      return choice === "allow" ? put(id, "allowed", { allowed_at: new Date().toISOString() }) : put(id, "declined", { allowed_at: null });
    },
    revoke: async (id) => { writes.push(`revoke:${id}`); await wait(); return put(id, "declined", { allowed_at: null, running_threads: 0 }); },
  };
}
