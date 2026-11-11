import { useState } from "react";
import type { AccountView } from "../../generated/AccountView";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { accountApi } from "./api";
import { Failure } from "./Card";
import { ConfirmItsYou } from "./ConfirmItsYou";
import { NETWORK_TEXT, problemOf } from "./problem";

const changes = (count: number) => `${count} settings ${count === 1 ? "change" : "changes"}`;

function Lists({ stops }: { stops: string[] }) {
  return (
    <div className="ac-two">
      <div>
        <b>Stays on this computer</b>
        <ul>
          <li>Threads, projects and the Crew ledger</li>
          <li>Your settings (kept as they are)</li>
          <li>Provider sign-ins</li>
        </ul>
      </div>
      <div>
        <b>Stops</b>
        <ul>{stops.map((line) => <li key={line}>{line}</li>)}</ul>
      </div>
    </div>
  );
}

/** Sign out of this computer. Needs no connection; unsynced settings changes are kept or discarded by choice. */
export function SignOutDialog({ view, onClose, onDone }: { view: AccountView; onClose(): void; onDone(): void }) {
  const waiting = view.sync.waiting;
  const offline = view.sync.kind === "offline";
  const [discard, setDiscard] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const run = async (syncFirst: boolean) => {
    setBusy(true);
    setProblem(null);
    try {
      if (syncFirst) await accountApi.syncNow();
      await accountApi.signOut(syncFirst ? false : discard);
      onDone();
    } catch (error) {
      const found = problemOf(error);
      setProblem(found.kind === "network" ? NETWORK_TEXT : "Couldn’t sign out. Nothing was changed.");
      setBusy(false);
    }
  };

  const stops = ["Settings sync", "Access to your account on this computer", `This device’s keys, removed from the ${view.platform === "windows" ? "Windows credential store" : view.platform === "macos" ? "macOS Keychain" : "system keychain"}`];
  const choice = (
    <fieldset className="ac-choice" disabled={busy}>
      <legend className="sr-only">What happens to the changes that haven’t synced</legend>
      <label>
        <input type="radio" name="unsynced" checked={!discard} onChange={() => setDiscard(false)} />
        <span><b>Keep unsynced changes on this device</b><small>They stay here and sync the next time you sign in.</small></span>
      </label>
      <label>
        <input type="radio" name="unsynced" checked={discard} onChange={() => setDiscard(true)} />
        <span><b>Discard them</b><small>They are deleted from this device and never reach your other devices.</small></span>
      </label>
    </fieldset>
  );

  let title = "Sign out of this computer?";
  let lead = "Signing out removes this device’s keys from the Keychain. Orbyn keeps working here without an account. Other devices stay signed in.";
  if (offline) {
    title = "Sign out while offline?";
    lead = "You’re offline. This computer will sign out now; its entry in your device list stays until you remove it from another device.";
  } else if (waiting > 0) {
    title = `Sign out with ${waiting} ${waiting === 1 ? "change" : "changes"} not synced?`;
    lead = `${changes(waiting)} ${waiting === 1 ? "hasn’t" : "haven’t"} reached your other devices yet. Choose what happens to them. Signing out also removes this device’s keys from the Keychain.`;
  }
  return (
    <Dialog
      title={title}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          {waiting > 0 && !offline ? <Button onClick={() => void run(true)} disabled={busy}>Sync now, then sign out</Button> : null}
          <Button tone="danger" onClick={() => void run(false)} disabled={busy}>Sign out</Button>
        </>
      }
    >
      <p>{lead}</p>
      {offline || waiting === 0 ? <Lists stops={stops} /> : null}
      {offline && waiting > 0 ? <p className="ac-sub">{changes(waiting)} {waiting === 1 ? "hasn’t" : "haven’t"} synced.</p> : null}
      {waiting > 0 ? choice : null}
      {problem ? <Failure>{problem}</Failure> : null}
    </Dialog>
  );
}

/** Sign out everywhere: the explanation, then the password, then the action. While deletion is scheduled it only explains. */
export function SignOutEverywhere({ view, devices, onClose, onDone, onThisComputer }: { view: AccountView; devices: number | null; onClose(): void; onDone(): void; onThisComputer(): void }) {
  const [confirming, setConfirming] = useState(false);
  if (view.deletion_scheduled) {
    return (
      <Dialog
        title="Deletion is scheduled."
        onClose={onClose}
        footer={
          <>
            <Button onClick={onClose}>Close</Button>
            <Button onClick={onThisComputer}>Sign out of this computer</Button>
          </>
        }
      >
        <p>Orbyn can’t sign out every session right now. Sign out everywhere is off while your account is waiting to be deleted. Signing out of this computer only still works.</p>
      </Dialog>
    );
  }
  if (confirming) {
    return (
      <ConfirmItsYou
        action="sign_out_everywhere"
        onConfirmed={async () => {
          await accountApi.signOutEverywhere();
          onDone();
        }}
        onClose={onClose}
      />
    );
  }
  return (
    <Dialog
      title="Sign out everywhere?"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button tone="danger" onClick={() => setConfirming(true)}>Sign out everywhere</Button>
        </>
      }
    >
      <p>Every session of your account ends, this one too, and this computer goes back to the signed-out state. Each device keeps its own data and works without an account.</p>
      <div className="ac-two">
        <div>
          <b>Stays</b>
          <ul>
            <li>Threads, projects and settings on each device</li>
            <li>Provider sign-ins</li>
            <li>Each device in your device list, until someone removes it</li>
          </ul>
        </div>
        <div>
          <b>Stops</b>
          <ul>
            <li>Every session of your account, this one too</li>
            <li>{devices === null ? "Settings sync on every device" : `Settings sync on all ${devices} devices`}</li>
          </ul>
        </div>
      </div>
    </Dialog>
  );
}
