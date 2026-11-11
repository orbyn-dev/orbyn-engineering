import { useState } from "react";
import type { AccountDevice } from "../../generated/AccountDevice";
import type { AccountView } from "../../generated/AccountView";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { accountApi } from "./api";
import { Failure } from "./Card";
import { ConfirmItsYou } from "./ConfirmItsYou";
import { DeviceRows, DevicesNote } from "./DeviceRows";
import { useAfter } from "./useCountdown";
import { useDevices } from "./useDevices";

/** Account › Devices: who is signed in, and Remove for the ones that are not this computer. */
export function DevicesPage({ view }: { view: AccountView }) {
  const devices = useDevices();
  const [asking, setAsking] = useState<AccountDevice | null>(null);
  const [confirming, setConfirming] = useState<AccountDevice | null>(null);
  const [removed, setRemoved] = useState<string | null>(null);
  const skeleton = useAfter(devices.status === "loading");
  const failed = devices.status === "error";
  const list = devices.data;
  const syncedAt = view.sync.last_synced ? view.sync.last_synced * 1000 : null;

  return (
    <div className="ac-devpage">
      <h2 className="ac-pagetitle">Devices</h2>
      {failed ? (
        <div className="ac-loadfail" role="alert">
          <Failure>Couldn’t load your devices.</Failure>
          <small>Orbyn didn’t answer.</small>
          <Button small onClick={devices.reload}>Try again</Button>
        </div>
      ) : null}
      {removed ? <p className="ac-done" role="status"><b>{removed} removed.</b> It was signed out and stopped syncing.</p> : null}
      <p className="ac-sub">Devices signed in to your account. Each one shows the Orbyn app it signs in to.</p>
      {list ? (
        <div className="ac-last" data-stale={failed || undefined}>
          <DeviceRows devices={list} syncedAt={syncedAt} onRemove={failed ? undefined : setAsking} />
        </div>
      ) : skeleton ? (
        <div className="ac-sk" aria-hidden>
          <div className="ac-skr"><i /><i /></div>
          <div className="ac-skr"><i /><i /></div>
        </div>
      ) : null}
      <DevicesNote platform={view.platform} />
      {list && list.length === 1 && !failed ? (
        <p className="ac-sub"><b>This is the only device signed in.</b> To add another, sign in to Orbyn on it, then approve it from here or any trusted device.</p>
      ) : null}

      {asking ? (
        <Dialog
          title={`Remove ${asking.name}?`}
          onClose={() => setAsking(null)}
          footer={
            <>
              <Button onClick={() => setAsking(null)}>Cancel</Button>
              <Button tone="danger" onClick={() => { setConfirming(asking); setAsking(null); }}>Remove device</Button>
            </>
          }
        >
          <p>It will be signed out of your account and stop syncing. Threads, code and settings already on it stay on that computer.</p>
        </Dialog>
      ) : null}
      {confirming ? (
        <ConfirmItsYou
          action="remove_device"
          deviceId={confirming.id}
          subject={`To remove ${confirming.name}`}
          onConfirmed={async () => {
            await accountApi.removeDevice(confirming.id);
            setRemoved(confirming.name);
            devices.reload();
          }}
          onClose={() => setConfirming(null)}
        />
      ) : null}
    </div>
  );
}
