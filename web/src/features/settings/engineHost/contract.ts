/**
 * The engine host's status and the sign-in storage choice, as the settings screens read them.
 *
 * PLANNED CONTRACT. The Rust side of these routes is not merged: no type here is generated, and the only
 * implementation of `EngineHostApi` that exists is the mock in `web/mock/engineHost.ts`. When the routes land,
 * replace these hand-written types with the generated ones in the same commit that adds the real client.
 *
 * Nothing in this file can carry a secret. A sign-in is never a field: the storage choice is a word and a date.
 */

/** The nine labels a host row can carry (the protocol's Handshake and Crash and recovery sections). */
export type HostState =
  | "not_started" | "starting" | "running" | "stopped" | "crashed"
  | "restarting" | "cant_start" | "version_mismatch" | "not_used";

/** The closed set of error kinds the host sends (protocol, Errors). Absent means unknown. */
export type HostErrorKind =
  | "auth" | "rate_limit" | "network" | "invalid_request" | "unsupported" | "cancelled" | "engine" | "host" | "session_lost";

/** One link of an error's cause chain, outermost first. The stack is deliberately not here. */
export interface HostCause {
  name: string;
  message: string;
  code?: string | null;
}

export interface HostError {
  kind?: HostErrorKind | null;
  /** The short, already redacted sentence. */
  message: string;
  /** Up to 8 causes; a loop ends with the message `[Circular error cause]`. */
  causes: HostCause[];
}

/** One engine host: one per family, or per instance where the engine reads process-wide settings. */
export interface HostRow {
  /** Matches `EngineStatus.id`, so a provider's detail page finds its host. */
  id: string;
  name: string;
  state: HostState;
  /** One host serves every thread of the family (Cursor). */
  pooled?: boolean;
  /** ISO time the host started running. */
  since?: string | null;
  /** ISO time it stopped or crashed. */
  stopped_at?: string | null;
  host_version?: string | null;
  protocol?: number | null;
  /** The protocol number this app speaks, for a mismatch. */
  app_protocol?: number | null;
  sdk?: { name: string; version: string } | null;
  /** The host's own `hello` list. */
  capabilities?: string[];
  restart?: { attempt: number; limit: number; next_try_seconds: number | null } | null;
  failed_threads?: number | null;
  /** For can't start: how many times it stopped inside the window. */
  stops_in_window?: number | null;
  last_error?: HostError | null;
  /** Only a family marked unavailable waits for the person. */
  can_retry: boolean;
}

/** A host crash for Maintenance diagnostics, the only place a stack is ever shown. */
export interface HostCrash {
  id: string;
  at: string;
  message: string;
  kind?: HostErrorKind | null;
  restart?: { attempt: number; limit: number } | null;
  failed_threads: number;
  causes: HostCause[];
  stack?: string | null;
}

/** What the person chose for one engine's sign-in. `removing`, `locked` and `denied` are never chosen, only reached. */
export type StorageChoice = "not_asked" | "allowed" | "declined" | "removing" | "locked" | "denied";

export interface SignInStorage {
  /** `EngineStatus.id` of the engine. */
  engine_id: string;
  name: string;
  choice: StorageChoice;
  /** ISO time the person allowed it. */
  allowed_at?: string | null;
  /** Threads running on this engine now; revoking stops them. */
  running_threads: number;
}

/** A refused keychain write: the service answers with one of these codes and nothing else about why. */
export const KEYCHAIN_LOCKED = "keychain_locked";
export const KEYCHAIN_DENIED = "keychain_denied";

/**
 * The one door the screens use. `null` from a read means the service reports no engine host data, and the
 * screens then draw nothing. Every write is one request for one click.
 */
export interface EngineHostApi {
  hosts(signal?: AbortSignal): Promise<HostRow[] | null>;
  crashes(signal?: AbortSignal): Promise<HostCrash[]>;
  /** The engines that may keep a sign-in in the keychain, or `null` when the service has none to offer. */
  storage(signal?: AbortSignal): Promise<SignInStorage[] | null>;
  /** Start a family that is marked unavailable. */
  retry(hostId: string): Promise<void>;
  /** Keep (`allow`) or never keep (`decline`) this engine's sign-in in the system keychain. Rejects with `KEYCHAIN_LOCKED` or `KEYCHAIN_DENIED`. */
  choose(engineId: string, choice: "allow" | "decline"): Promise<SignInStorage>;
  /** Delete the stored item and stop the threads running on the engine. */
  revoke(engineId: string): Promise<SignInStorage>;
}
