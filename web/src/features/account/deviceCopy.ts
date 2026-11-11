import type { AccountDevice } from "../../generated/AccountDevice";
import { relativeTime } from "../../lib/format";

const day = (iso: string) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });

/** "Orbyn Engineering" or "Orbyn Planner": the app a device signs in to. */
export const appLabel = (device: AccountDevice) => (device.app === "engineering" ? "Orbyn Engineering" : "Orbyn Planner");

/** "Trusted since 12 Jul 2026 · 4 months ago · Seen 2 min ago". This device says when it last synced instead. */
export function deviceLine(device: AccountDevice, syncedAt: number | null, now = Date.now()): string {
  const parts = [`Trusted since ${day(device.trusted_since)} · ${relativeTime(device.trusted_since, now)}`];
  if (device.current) {
    if (syncedAt !== null) parts.push(now - syncedAt < 60_000 ? "Synced now" : `Synced ${relativeTime(new Date(syncedAt).toISOString(), now)}`);
  } else if (device.last_seen) {
    parts.push(`Seen ${relativeTime(device.last_seen, now).toLowerCase()}`);
  }
  return parts.join(" · ");
}
