/**
 * The typed client for the Snap Shot shell commands (docs/snapshots.md). The page asks the desktop shell through the same bridge
 * `personKey.ts` uses and never calls the service for a capture. Without a bridge, or when `snapshot_state` fails or answers
 * `supported: false`, `readSnapShotGate` answers null and every Snap Shot control is absent.
 */
import { bridge } from "../../api/bridge";
import {
  isSnapShotState,
  isSnapShotTake,
  type SnapShotEventName,
  type SnapShotSetupAction,
  type SnapShotSettings,
  type SnapShotState,
  type SnapShotTake,
} from "./types";

export interface SnapShotTransport {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(event: SnapShotEventName, handler: (payload: unknown) => void): Promise<() => void>;
}

export interface SnapShotClient {
  state(): Promise<SnapShotState>;
  configure(settings: SnapShotSettings): Promise<SnapShotState>;
  capture(): Promise<{ id: string }>;
  take(id: string): Promise<SnapShotTake>;
  discard(id: string): Promise<null>;
  setup(action: SnapShotSetupAction): Promise<SnapShotState>;
  on(event: SnapShotEventName, handler: (payload: unknown) => void): Promise<() => void>;
}

export interface SnapShotGate { client: SnapShotClient; state: SnapShotState }

/** A dev-only mock of the shell, enough for the tests and the preview page to reach every frame. */
export interface SnapShotMock extends SnapShotTransport {
  readonly calls: { command: string; args?: Record<string, unknown> }[];
  readonly configured: SnapShotSettings[];
  readonly emits: { event: SnapShotEventName; payload?: unknown }[];
  setState(patch: Partial<SnapShotState>): void;
  setSettings(patch: Partial<SnapShotSettings>): void;
  setCapture(mode: "ready" | "failed" | "excluded" | "pending", detail?: { reason?: string; app_name?: string }): void;
  setTake(take: Partial<SnapShotTake>): void;
  setFrame(frame: string): void;
  emit(event: SnapShotEventName, payload?: unknown): void;
}

const stateOf = (value: unknown): SnapShotState => {
  if (!isSnapShotState(value)) throw new Error("The shell answered a state this page cannot read.");
  return value;
};

export function clientOf(transport: SnapShotTransport): SnapShotClient {
  return {
    state: async () => stateOf(await transport.invoke("snapshot_state")),
    configure: async (settings) => stateOf(await transport.invoke("snapshot_configure", settings as unknown as Record<string, unknown>)),
    capture: async () => {
      const answer = await transport.invoke("snapshot_capture");
      if (typeof answer !== "object" || answer === null || typeof (answer as { id?: unknown }).id !== "string") {
        throw new Error("The shell did not name the capture.");
      }
      return { id: (answer as { id: string }).id };
    },
    take: async (id) => {
      const answer = await transport.invoke("snapshot_take", { id });
      if (!isSnapShotTake(answer)) throw new Error("The shell answered no image for this capture.");
      return answer;
    },
    discard: async (id) => {
      await transport.invoke("snapshot_discard", { id });
      return null;
    },
    setup: async (action) => stateOf(await transport.invoke("snapshot_setup", { action })),
    on: (event, handler) => transport.listen(event, handler),
  };
}

type EventBridge = {
  invoke?: (command: string, args?: Record<string, unknown>) => unknown;
  transformCallback?: (callback: (event: unknown) => void, once?: boolean) => number;
};

/** The bridge as the desktop shell exposes it, or null in a plain browser. */
function tauriTransport(): SnapShotTransport | null {
  const internals = bridge() as EventBridge | undefined;
  const invoke = internals?.invoke;
  if (typeof invoke !== "function") return null;
  return {
    invoke: async (command, args) => invoke.call(internals, command, args),
    listen: async (event, handler) => {
      const transform = internals?.transformCallback;
      if (typeof transform !== "function") return () => undefined;
      const id = transform.call(internals, (message) => {
        const payload = typeof message === "object" && message !== null && "payload" in message
          ? (message as { payload: unknown }).payload
          : message;
        handler(payload);
      });
      await invoke.call(internals, "plugin:event|listen", { event, handler: id });
      return () => {
        void Promise.resolve(invoke.call(internals, "plugin:event|unlisten", { event, eventId: id })).catch(() => undefined);
      };
    },
  };
}

let installed: SnapShotTransport | null | undefined;
let previewInstall: Promise<void> | null = null;

/** Tests and the preview install the mock here; null disables the transport, and `resetSnapshotShellForTests` returns to the bridge. */
export function installSnapshotTransport(transport: SnapShotTransport | null): void {
  installed = transport;
}

/** The installed dev-only mock, when this page has one; the preview uses it to reach each frame. */
export function installedSnapshotMock(): SnapShotMock | null {
  return installed !== null && installed !== undefined && typeof (installed as SnapShotMock).setFrame === "function"
    ? installed as SnapShotMock
    : null;
}

/** The transport this page has now: an installed mock, else the desktop bridge. */
export function snapshotTransport(): SnapShotTransport | null {
  return installed === undefined ? tauriTransport() : installed;
}

/** Waits for the dev-only preview's mock to be installed, then answers the transport. */
export async function snapshotTransportReady(): Promise<SnapShotTransport | null> {
  try {
    if (previewInstall) await previewInstall;
  } catch {
    installed = null;
  }
  return snapshotTransport();
}

/** Reads the shell's state and decides whether the whole feature exists. A throw or supported:false hides it. */
export async function readSnapShotGate(): Promise<SnapShotGate | null> {
  const transport = await snapshotTransportReady();
  if (!transport) return null;
  try {
    const state = stateOf(await transport.invoke("snapshot_state"));
    return state.supported ? { client: clientOf(transport), state } : null;
  } catch {
    return null;
  }
}

/** Forgets an installed mock and the preview import. For tests. */
export function resetSnapshotShellForTests(): void {
  installed = undefined;
  previewInstall = null;
}

// Dev only: `#/preview/snapshot` installs the mock so every frame can be reached before the shell exists. The production build
// defines the flag false, so this branch and the mock chunk are not emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/snapshot")) {
  previewInstall = import("../../../mock/snapshot").then((module) => {
    installed = module.createSnapshotMock();
  });
}
