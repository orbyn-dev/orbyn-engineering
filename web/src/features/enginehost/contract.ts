import type { EngineHostCrash } from "../../generated/EngineHostCrash";
import type { EngineHostErrorKind } from "../../generated/EngineHostErrorKind";
import type { EngineHostNotUsed } from "../../generated/EngineHostNotUsed";
import type { EngineHostState } from "../../generated/EngineHostState";

/**
 * The engine host's status and the API key storage choice, as the settings screens read them.
 *
 * The host status is REAL: `GET /api/engine-host` (Local read) answers one `EngineHostStatus` per family, and
 * `hostStatus.ts` turns each into the `HostRow` below. Stop and restart are Person routes
 * (`POST /api/engine-host/{family}/stop` and `/restart`); Try again is the restart. A host with a background task open answers 409
 * `background_open`, and only a second request with `stopBackground: true`, sent after the person confirmed, goes through.
 * A field marked GUESSED is one the real client does not fill; a screen draws one only when it is there. The mock fills them.
 *
 * Nothing a screen reads can carry a secret: the API key storage choice is a word and a date. The key goes outward once, as the argument of `saveKey`.
 */

/** The nine labels a host row can carry: the generated `EngineHostState`. A word the service adds later is shown as Unknown. */
export type HostState = EngineHostState;

/** PLANNED (not in slice 1): the closed set of run error kinds the host sends (protocol, Errors). Absent means unknown. */
export type HostErrorKind =
  | "auth" | "rate_limit" | "network" | "invalid_request" | "unsupported" | "cancelled" | "engine" | "host" | "session_lost";

/** PLANNED: one link of an error's cause chain, outermost first. The stack is deliberately not here. */
export interface HostCause {
  name: string;
  message: string;
  code?: string | null;
}

export interface HostError {
  /** PLANNED: the run error kind, not the crash's `error_kind`. */
  kind?: HostErrorKind | null;
  /** The short, already redacted sentence. */
  message: string;
  /** PLANNED: up to 8 causes; a loop ends with the message `[Circular error cause]`. */
  causes: HostCause[];
}

/**
 * One engine host: one per family. The first group of fields is what slice 1 returns; the rest is PLANNED or mock-only.
 * Text the service chose (`name`, `detail`, `action`, the crash message and stderr tail) is cut and drawn as text, never markup.
 */
export interface HostRow {
  /** The family (`EngineHostStatus.family`), which matches `EngineStatus.id`, so a provider's detail page finds its host. */
  id: string;
  /** The family's display name; the service sends only the family, so this is the family's own name or the family itself. */
  name: string;
  state: HostState;
  host_version?: string | null;
  /** Restarts after a crash since the app started. */
  restart_count?: number;
  /** The last time the host failed. */
  last_crash?: EngineHostCrash | null;
  /** The host's protocol number, for a mismatch (`protocol_host`). */
  protocol?: number | null;
  /** The protocol number this app speaks, for a mismatch (`protocol_app`). */
  app_protocol?: number | null;
  /** For `not_used`: why the family does not use the host. */
  not_used?: EngineHostNotUsed | null;
  /** A plain sentence the service gives for a state that stops runs. */
  detail?: string | null;
  /** What the person can do, when there is one thing (`Update Orbyn`). */
  action?: string | null;
  /** The crash's time, for a row that says when it stopped (`last_crash.at`, or the mock's `stopped_at`). */
  stopped_at?: string | null;
  /** The crash as a sentence plus (PLANNED) a cause chain; derived from `last_crash` for a real row. */
  last_error?: HostError | null;
  /** Only a family marked unavailable (`cant_start`) waits for the person: the real client sets it for that state. */
  can_retry: boolean;
  /** One host process serves every thread of the family. */
  pooled?: boolean;
  /** ISO time the live host started running. */
  since?: string | null;
  /** The SDK package and version the live host's hello named. */
  sdk?: { name: string; version: string } | null;
  /** The requests the live host's hello says it implements. */
  capabilities?: string[];
  /** The automatic restart the family waits for, when it is restarting. */
  restart?: { attempt: number; limit: number; next_try_seconds: number | null } | null;
  failed_threads?: number | null;
  /** For can't start, how many times the host stopped inside the window. */
  stops_in_window?: number | null;
  /** GUESSED SHAPE: threads running on this host now, for the Stop question in Maintenance. */
  running_threads?: number;
  /** The app has a Rust driver this family's runs can use when the host cannot start. */
  has_driver?: boolean;
  /** The host answered that this build carries no SDK for the family. */
  sdk_missing?: boolean;
}

/** A host crash for Maintenance diagnostics: the generated `EngineHostCrash` with the family it belongs to. No stack exists. */
export interface HostCrash {
  /** Family and time, so a second crash of the family is another row. */
  id: string;
  family: string;
  at: string;
  message: string;
  error_kind: EngineHostErrorKind;
  exit_code?: number;
  signal?: number;
  /** Redacted and capped at 1024 characters by the core; drawn as plain text. */
  stderr_tail: string;
}

/**
 * Where an engine's API key stands. `not_asked`, `saved`, `declined` and `rejected` are what the service reports;
 * `opted_in_no_key` is the service's own status (opted in, no key saved: a key write failed after the opt-in, or the key was removed);
 * `saving`, `removing`, `locked`, `denied` and `remove_failed` are only ever reached by a click on this screen.
 * `rejected` is the engine reporting an auth error while using the saved key.
 */
export type StorageChoice = "not_asked" | "saved" | "declined" | "rejected" | "saving" | "removing" | "locked" | "denied" | "remove_failed" | "opted_in_no_key";

/** One engine's API key row. It is a state and a date: the key itself is never a field, in either direction of a read. */
export interface KeyStorage {
  /** `EngineStatus.id` of the engine. */
  engine_id: string;
  name: string;
  choice: StorageChoice;
  /** ISO time the key was saved. */
  saved_at?: string | null;
  /** Only on the answer to `removeKey`: how many of the engine's running threads the service stopped with the key. */
  stopped?: number | null;
}

/** The service's code for a stop or restart refused because the host has a background task open. */
export const BACKGROUND_OPEN = "background_open";

/** What a stop or restart asks beyond the host: `stopBackground` is sent only after the person was told background work would end. */
export interface HostControlOptions { stopBackground?: boolean }

/** A refused keychain write: the service answers with one of these codes and nothing else about why. */
export const KEYCHAIN_LOCKED = "keychain_locked";
export const KEYCHAIN_DENIED = "keychain_denied";

/**
 * The one door the screens use. `null` from a read means the service reports no engine host data, and the
 * screens then draw nothing. Every write is one request for one click.
 */
export interface EngineHostApi {
  /** False when the service has no stop or restart route: the screens then draw no button for them. Absent means true (the real client and the mock). */
  writable?: boolean;
  hosts(signal?: AbortSignal): Promise<HostRow[] | null>;
  crashes(signal?: AbortSignal): Promise<HostCrash[]>;
  /** The engines that may keep an API key in the keychain, or `null` when the service has none to offer. */
  storage(signal?: AbortSignal): Promise<KeyStorage[] | null>;
  /**
   * Start a family that is marked unavailable, or restart a running one (which ends its threads like `stop`).
   * The real route must be Person-class (ENFORCED in `web/src/api/personKey.ts` and the route class table in `crates/orbynd`): it changes what runs.
   */
  retry(hostId: string, options?: HostControlOptions): Promise<void>;
  /** Stop a running or restarting host; the threads on it end with the host's own sentence. The real route must be Person-class (ENFORCED). */
  stop(hostId: string, options?: HostControlOptions): Promise<void>;
  /** Never keep this engine's API key (a decision, kept). The real route must be Person-class (ENFORCED). */
  decline(engineId: string): Promise<KeyStorage>;
  /**
   * Save the key in the system keychain. This is the one place a secret crosses the contract, and only outward: the argument is
   * passed once, never stored, echoed, logged or read back. Rejects with `KEYCHAIN_LOCKED` or `KEYCHAIN_DENIED`.
   * The real route must be Person-class (ENFORCED), carry the key in the JSON body and never in the path or query, and must not
   * echo the key in any reply or error text.
   */
  saveKey(engineId: string, key: string): Promise<KeyStorage>;
  /** Turn the person's opt-in off without recording a decline: the row goes back to not asked. Person-class (ENFORCED). */
  removeOptIn(engineId: string): Promise<KeyStorage>;
  /** Delete the key Orbyn saved; the engine's own sign-in is untouched and running threads keep going. The real route must be Person-class (ENFORCED). */
  removeKey(engineId: string): Promise<KeyStorage>;
  /** Ask the keychain again after it was locked or refused. Changes nothing; answers the row as the service now has it. */
  retryKeychain(engineId: string): Promise<KeyStorage>;
}
