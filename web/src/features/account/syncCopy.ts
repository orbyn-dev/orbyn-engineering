import type { AccountSyncView } from "../../generated/AccountSyncView";

/** The word the sync line shows, and its tone: quiet when all is well, amber to ask, red only when the server refused a change. */
export type SyncTone = "quiet" | "bad";
export interface SyncLine {
  label: string;
  tone: SyncTone;
  /** A check mark in front: everything is synced. */
  done: boolean;
}

const refused = (sync: AccountSyncView) => sync.kind === "paused" && (sync.reason === "zone_failed" || sync.reason === "too_large");

export function syncLine(sync: AccountSyncView): SyncLine {
  if (refused(sync)) return { label: "Couldn’t sync", tone: "bad", done: false };
  switch (sync.kind) {
    case "paused":
      return { label: "Paused", tone: "quiet", done: false };
    case "offline":
      return { label: sync.waiting > 0 ? `Offline · ${sync.waiting} waiting` : "Offline", tone: "quiet", done: false };
    case "syncing":
    case "signing_in":
    case "throttled":
      return { label: "Syncing…", tone: "quiet", done: false };
    case "online":
      return sync.waiting > 0 ? { label: "Syncing…", tone: "quiet", done: false } : { label: "Synced", tone: "quiet", done: true };
    case "not_connected":
      return { label: "Not connected", tone: "quiet", done: false };
    case "signed_out":
      return { label: "Signed out", tone: "quiet", done: false };
  }
}

/** Whether a sync is running now: the row says "Syncing…" with no count (the service does not report one), and only Pause is offered. */
export function isSyncing(sync: AccountSyncView): boolean {
  if (refused(sync)) return false;
  return sync.kind === "syncing" || sync.kind === "signing_in" || sync.kind === "throttled" || (sync.kind === "online" && sync.waiting > 0);
}

export function lastSynced(sync: AccountSyncView): string | null {
  if (sync.last_synced === undefined || sync.last_synced === null) return null;
  return new Date(sync.last_synced * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

const waitingText = (count: number) => `${count} ${count === 1 ? "change" : "changes"} waiting`;

/** The Settings sync row on the Account page. */
export function syncRowText(sync: AccountSyncView, unreachable = false): string {
  const last = lastSynced(sync);
  const tail = last ? ` · last synced ${last}` : "";
  if (unreachable) return `Couldn’t check${tail}`;
  if (refused(sync)) return `Couldn’t sync${sync.waiting > 0 ? ` · ${waitingText(sync.waiting)}` : ""}${tail}`;
  switch (sync.kind) {
    case "paused":
      return `Paused${sync.waiting > 0 ? ` · ${waitingText(sync.waiting)}` : ""}${tail}`;
    case "offline":
      return `Offline${sync.waiting > 0 ? ` · ${waitingText(sync.waiting)}` : ""}${tail}`;
    case "syncing":
    case "signing_in":
    case "throttled":
      return "Syncing…";
    case "online":
      return sync.waiting > 0 ? "Syncing…" : `Up to date${tail}`;
    case "not_connected":
      return "Not connected · this computer’s settings stay here";
    case "signed_out":
      return "Signed out";
  }
}

/**
 * When This device last synced, for its line in the device list: only while everything is synced. A device that is
 * paused, offline, syncing or couldn't sync says nothing about its last sync there (the Settings sync row does).
 */
export function deviceSyncedAt(sync: AccountSyncView): number | null {
  const done = sync.kind === "online" && sync.waiting === 0 && !refused(sync);
  return done && sync.last_synced ? sync.last_synced * 1000 : null;
}
