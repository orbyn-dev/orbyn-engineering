import { ApiFailure, request } from "../../api/client";
import type { EngineCredentialKeyInput } from "../../generated/EngineCredentialKeyInput";
import type { EngineCredentialsOptIn } from "../../generated/EngineCredentialsOptIn";
import type { EngineCredentialsRevoked } from "../../generated/EngineCredentialsRevoked";
import type { EngineCredentialsState } from "../../generated/EngineCredentialsState";
import type { EngineCredentialsStatus } from "../../generated/EngineCredentialsStatus";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type KeyStorage, type StorageChoice } from "./contract";

/**
 * The real client for API key storage: `/api/settings/engine-credentials/{engine}` (docs/architecture.md). The status read is `Local`;
 * the opt-in, the key write, the removal and the retry are Person routes, so `request` sends the person header on those four
 * (`web/src/api/personKey.ts`). The key travels in one PUT body and nowhere else: not a path, a query, a header, a log or an error.
 * Not yet run against a real service: only stubbed fetch tests cover it.
 */

/**
 * The engines the service keeps a key for (`ENGINES` in `crates/orbyn-core/src/credentials/mod.rs`, which has no listing route).
 * An engine the service does not know answers 404 and is left out, so a longer list here can only show less, never more.
 */
const OFFERED: readonly { engine_id: string; name: string }[] = [{ engine_id: "cursor", name: "Cursor" }];

const path = (engine: string, tail = "") => `/api/settings/engine-credentials/${encodeURIComponent(engine)}${tail}`;

const STATES = new Set<string>(["off", "unavailable", "blocked", "not_saved", "saved"]);

/** What the service says about each state, in the words the row already has. The service does not tell "not asked" from "declined". */
function choiceOf(state: EngineCredentialsState, declined: boolean): StorageChoice {
  switch (state) {
    case "saved": return "saved";
    case "blocked": return "locked";
    case "unavailable": return "denied";
    case "off": return declined ? "declined" : "not_asked";
    default: return "not_asked";
  }
}

/** The key as it could appear in an error: raw, URL-encoded, JSON-escaped and base64. */
function forms(key: string): string[] {
  const out = new Set<string>([key, encodeURIComponent(key), JSON.stringify(key).slice(1, -1)]);
  try { out.add(btoa(key)); } catch { /* not Latin-1: base64 of the raw text cannot be built here */ }
  try { out.add(btoa(String.fromCharCode(...new TextEncoder().encode(key)))); } catch { /* too long to spread */ }
  return [...out].filter((form) => form !== "");
}

function scrub(text: string, key: string): string {
  let clean = text;
  for (const form of forms(key)) clean = clean.split(form).join("[removed]");
  return clean;
}

export interface CredentialsApi {
  storage(signal?: AbortSignal): Promise<KeyStorage[] | null>;
  decline(engineId: string): Promise<KeyStorage>;
  saveKey(engineId: string, key: string): Promise<KeyStorage>;
  removeKey(engineId: string): Promise<KeyStorage>;
  retryKeychain(engineId: string): Promise<KeyStorage>;
}

export function createCredentialsApi(): CredentialsApi {
  // What the service last said, per engine id. Keys are ids the caller or the service chose, so a Map, never an object.
  const optedIn = new Map<string, boolean>();
  const declined = new Set<string>();
  const nameOf = (engine: string) => OFFERED.find((row) => row.engine_id === engine)?.name ?? engine;

  const row = (engine: string, status: EngineCredentialsStatus | EngineCredentialsRevoked): KeyStorage => {
    // An answer that is not a status (a proxy's page, another route's body) is no row, never a guess.
    if (!status || typeof status !== "object" || !STATES.has(status.state) || typeof status.opted_in !== "boolean") {
      throw new ApiFailure("bad_answer", "The service answered something this screen does not know.", 0);
    }
    optedIn.set(engine, status.opted_in);
    if (status.opted_in) declined.delete(engine);
    const next: KeyStorage = { engine_id: engine, name: nameOf(engine), choice: choiceOf(status.state, declined.has(engine)) };
    return "stopped" in status ? { ...next, stopped: status.stopped } : next;
  };

  const optIn = async (engine: string, opted_in: boolean) => {
    const body: EngineCredentialsOptIn = { opted_in };
    return request<EngineCredentialsStatus>("PUT", path(engine), body);
  };

  /** Blocked and unavailable keychains are the two answers the screen has words for; everything else keeps its own code. */
  const refused = (failure: unknown, key?: string): never => {
    const clean = (text: string) => (key === undefined ? text : scrub(text, key));
    if (failure instanceof ApiFailure) {
      const code = failure.code === "keychain_blocked" ? KEYCHAIN_LOCKED : failure.code === "keychain_unavailable" ? KEYCHAIN_DENIED : failure.code;
      // `detail` is dropped: nothing but the code, the status and the cleaned sentence leaves.
      throw new ApiFailure(code, clean(failure.message), failure.status);
    }
    throw new Error(clean(failure instanceof Error ? failure.message : "The request failed."));
  };

  return {
    async storage(signal) {
      const rows: KeyStorage[] = [];
      for (const offered of OFFERED) {
        try {
          rows.push(row(offered.engine_id, await request<EngineCredentialsStatus>("GET", path(offered.engine_id), undefined, signal)));
        } catch (failure) {
          // 404: this service does not keep a key for the engine, and the screens draw nothing for it.
          if (failure instanceof ApiFailure && failure.status === 404) continue;
          throw failure;
        }
      }
      return rows.length === 0 ? null : rows;
    },
    async decline(engine) {
      declined.add(engine);
      try {
        return row(engine, await optIn(engine, false));
      } catch (failure) {
        declined.delete(engine);
        return refused(failure);
      }
    },
    async saveKey(engine, key) {
      try {
        // The key route answers 409 `opt_in_off` until the person's opt-in is on, so the first save turns it on, once.
        if (optedIn.get(engine) !== true) row(engine, await optIn(engine, true));
        const body: EngineCredentialKeyInput = { key };
        return row(engine, await request<EngineCredentialsStatus>("PUT", path(engine, "/key"), body));
      } catch (failure) {
        return refused(failure, key);
      }
    },
    async removeKey(engine) {
      try {
        return row(engine, await request<EngineCredentialsRevoked>("DELETE", path(engine, "/key")));
      } catch (failure) {
        return refused(failure);
      }
    },
    async retryKeychain(engine) {
      try {
        return row(engine, await request<EngineCredentialsStatus>("POST", path(engine, "/retry")));
      } catch (failure) {
        return refused(failure);
      }
    },
  };
}
