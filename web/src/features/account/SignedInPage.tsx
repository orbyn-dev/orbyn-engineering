import { useState } from "react";
import { Button } from "../../ui/controls";
import type { AccountView } from "../../generated/AccountView";
import { navigate } from "../../product/state/route";
import type { AccountDevice } from "../../generated/AccountDevice";
import { accountApi } from "./api";
import { DeviceRows, DevicesNote } from "./DeviceRows";
import { pauseInfo } from "./pauseCopy";
import { isSyncing, syncRowText } from "./syncCopy";
import { useLayoutWidth } from "../../lib/useLayout";

/** The Account page lists a few devices and Manage devices lists them all: three, two in a narrow window. */
export const shownDevices = (layout: string) => (layout === "phone" || layout === "small" ? 2 : 3);

/** The person's account, and the one sync zone that exists. */
export function SignedInPage({
  view,
  stale = false,
  devices,
  onChanged,
  onError,
  onOpenDevices,
  onConnect,
  onSignOut,
  onSignOutEverywhere,
}: {
  view: AccountView;
  stale?: boolean;
  devices: AccountDevice[] | null;
  onChanged(): void;
  onError(message: string): void;
  onOpenDevices(): void;
  onConnect(): void;
  onSignOut(): void;
  onSignOutEverywhere(): void;
}) {
  const session = view.session;
  const [busy, setBusy] = useState(false);
  const shown = shownDevices(useLayoutWidth());
  if (!session) return null;
  const sync = view.sync;
  const pausedAny = sync.kind === "paused" && sync.reason !== "zone_failed" && sync.reason !== "too_large";
  const pause = pausedAny ? pauseInfo(sync, view.deletion_scheduled) : null;
  const offline = sync.kind === "offline";
  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
      onChanged();
    } catch {
      onError("Couldn’t change sync. Nothing was changed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="ac-id">
        <span className="ac-avb" aria-hidden>{session.email.slice(0, 1).toUpperCase()}</span>
        <div>
          <b>{session.email}</b>
          <span>Signed in on this device · {session.device_name}</span>
        </div>
      </div>
      <h2 className="ac-grp">Sync</h2>
      <div className="ac-row">
        <div>
          <b>Settings sync</b>
          <small>{stale ? syncRowText(sync, true) : syncRowText(sync)}</small>
          {pause?.off && !stale ? <small>{pause.off}</small> : null}
        </div>
        {view.connect !== "connected" ? (
          <Button small disabled={stale} onClick={onConnect}>Connect settings</Button>
        ) : pausedAny || (isSyncing(sync) && !stale) ? null : offline ? (
          <Button small disabled>Waiting for a connection</Button>
        ) : (
          <Button small disabled={busy || stale} onClick={() => void act(() => accountApi.syncNow())}>Sync now</Button>
        )}
        {view.connect !== "connected" ? null : pause ? (
          pause.action.kind === "resume" ? (
            <Button small disabled={busy || stale || !pause.action.enabled} onClick={() => void act(() => accountApi.resumeSync())}>Resume sync</Button>
          ) : pause.action.kind === "keybindings" ? (
            <Button small disabled={stale} onClick={() => navigate({ name: "settings", section: "keybindings" })}>Open keybindings</Button>
          ) : pause.action.kind === "retry" ? (
            <Button small disabled={busy || stale} onClick={() => void act(() => accountApi.syncNow())}>Try again</Button>
          ) : null
        ) : offline ? null : (
          <Button small disabled={busy || stale} onClick={() => void act(() => accountApi.pauseSync())}>Pause</Button>
        )}
      </div>
      <div className="ac-row">
        <div>
          <b>What syncs</b>
          <small>Appearance, keybindings, chat and diff preferences. Never threads, code, files or secrets.</small>
        </div>
        <span className="ac-aside">Settings only</span>
      </div>
      <div className="ac-row">
        <div>
          <b>Execution profiles</b>
          <small>Stay on each device: a profile sets a permission mode, tier and Autopilot mode, so another device never applies one automatically.</small>
        </div>
        <span className="ac-aside">This device only</span>
      </div>
      <h2 className="ac-grp">Devices{devices ? (devices.length > shown ? ` · ${shown} of ${devices.length}` : ` · ${devices.length}`) : ""}</h2>
      {devices ? <DeviceRows devices={devices.slice(0, shown)} syncedAt={view.sync.last_synced ? view.sync.last_synced * 1000 : null} /> : null}
      <button type="button" className="ac-lnk ac-manage" onClick={onOpenDevices} disabled={stale}>Manage devices</button>
      <DevicesNote platform={view.platform} />
      <h2 className="ac-grp">Account</h2>
      <div className="ac-row">
        <div>
          <b>Sign out</b>
          <small>Signs out on this computer only.</small>
        </div>
        <Button small onClick={onSignOut} disabled={stale}>Sign out</Button>
      </div>
      <div className="ac-row">
        <div>
          <b>Sign out everywhere</b>
          <small>{view.deletion_scheduled ? "Off while your account is waiting to be deleted." : "Ends every session of your account, this one too. Asks for your password."}</small>
        </div>
        <Button small onClick={onSignOutEverywhere} disabled={stale}>Sign out everywhere…</Button>
      </div>
    </>
  );
}
