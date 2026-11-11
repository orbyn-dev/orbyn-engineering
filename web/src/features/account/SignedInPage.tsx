import { useState } from "react";
import { Button } from "../../ui/controls";
import type { AccountView } from "../../generated/AccountView";
import { accountApi } from "./api";
import { syncRowText } from "./syncCopy";

/** The person's account, and the one sync zone that exists. */
export function SignedInPage({ view, onChanged, onError }: { view: AccountView; onChanged(): void; onError(message: string): void }) {
  const session = view.session;
  const [busy, setBusy] = useState(false);
  if (!session) return null;
  const sync = view.sync;
  const paused = sync.kind === "paused" && sync.reason === "by_person";
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
          <small>{syncRowText(sync)}</small>
        </div>
        {offline ? (
          <Button small disabled>Waiting for a connection</Button>
        ) : (
          <Button small disabled={busy} onClick={() => void act(() => accountApi.syncNow())}>Sync now</Button>
        )}
        <Button small disabled={busy} onClick={() => void act(() => (paused ? accountApi.resumeSync() : accountApi.pauseSync()))}>
          {paused ? "Resume" : "Pause"}
        </Button>
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
    </>
  );
}
