import { expect, it } from "vitest";
import type { AccountSyncView } from "../../generated/AccountSyncView";
import { deviceSyncedAt } from "./syncCopy";

const sync = (over: Partial<AccountSyncView>): AccountSyncView => ({ kind: "online", waiting: 0, last_synced: 1_700_000_000, ...over }) as AccountSyncView;

it("tells This device when it last synced only while everything is synced", () => {
  expect(deviceSyncedAt(sync({}))).toBe(1_700_000_000_000);
});

it("says nothing about the last sync while paused, offline, syncing, failed or with changes waiting", () => {
  expect(deviceSyncedAt(sync({ kind: "paused" }))).toBeNull();
  expect(deviceSyncedAt(sync({ kind: "paused", reason: "zone_failed" }))).toBeNull();
  expect(deviceSyncedAt(sync({ kind: "offline" }))).toBeNull();
  expect(deviceSyncedAt(sync({ kind: "syncing" }))).toBeNull();
  expect(deviceSyncedAt(sync({ waiting: 3 }))).toBeNull();
  expect(deviceSyncedAt(sync({ last_synced: undefined }))).toBeNull();
});
