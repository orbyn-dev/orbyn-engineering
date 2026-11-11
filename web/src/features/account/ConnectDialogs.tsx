import { Loader2 } from "lucide-react";
import { useState } from "react";
import type { AccountConnectPolicy } from "../../generated/AccountConnectPolicy";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { accountApi } from "./api";
import { Failure } from "./Card";
import { ConfirmItsYou } from "./ConfirmItsYou";
import { NETWORK_TEXT, problemOf } from "./problem";

const CHOICES: { policy: AccountConnectPolicy; title: string; body: string }[] = [
  { policy: "combine", title: "Combine them", body: "Settings on both sides are merged one setting at a time. If the same setting differs, the newer change wins." },
  { policy: "use_device", title: "Use this device’s settings", body: "Replace the account’s settings with the ones on this device." },
  { policy: "use_account", title: "Use the account’s settings", body: "Replace the settings on this device with the account’s." },
];

/** After the first sign-in on a computer that has settings: how to join them to the account's. Combine is the default. */
export function ConnectChoice({ onClose, onConnected }: { onClose(): void; onConnected(): void }) {
  const [policy, setPolicy] = useState<AccountConnectPolicy>("combine");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<AccountConnectPolicy | null>(null);

  if (confirming) {
    return (
      <ConfirmItsYou
        action="connect_settings"
        policy={confirming}
        subject={confirming === "use_device" ? "To use this device’s settings everywhere" : "To use the account’s settings on this device"}
        onConfirmed={async () => {
          await (confirming === "use_device" ? accountApi.connectUseDevice() : accountApi.connectUseAccount());
          onConnected();
        }}
        onClose={() => setConfirming(null)}
      />
    );
  }
  const fail = (error: unknown) => setProblem(problemOf(error).kind === "network" ? NETWORK_TEXT : "Couldn’t connect your settings. Nothing was changed.");
  const connect = async () => {
    if (policy !== "combine") return setConfirming(policy);
    setBusy(true);
    setProblem(null);
    try {
      await accountApi.connect();
      onConnected();
    } catch (error) {
      fail(error);
      setBusy(false);
    }
  };
  const later = async () => {
    setBusy(true);
    try {
      await accountApi.connectLater();
    } catch {
      // The choice stays open; nothing was changed.
    }
    onClose();
  };
  return (
    <Dialog
      title="Sync your settings?"
      onClose={() => void later()}
      busy={busy}
      footer={
        <>
          <Button onClick={() => void later()} disabled={busy}>Not now</Button>
          <Button tone="primary" onClick={() => void connect()} disabled={busy}>Connect</Button>
        </>
      }
    >
      <p>This device has settings of its own. Choose how to connect them to your account.</p>
      <div className="ac-two">
        <div>
          <b>Will sync</b>
          <ul><li>Appearance and theme</li><li>Keybindings</li><li>Chat and diff preferences</li></ul>
        </div>
        <div>
          <b>Stays on this device</b>
          <ul><li>Threads and messages</li><li>Code and files</li><li>Secrets and provider sign-ins</li><li>Terminal history</li></ul>
        </div>
      </div>
      <fieldset className="ac-choice" disabled={busy}>
        <legend className="sr-only">How to connect your settings</legend>
        {CHOICES.map((choice) => (
          <label key={choice.policy}>
            <input type="radio" name="connect-policy" checked={policy === choice.policy} onChange={() => setPolicy(choice.policy)} />
            <span><b>{choice.title}</b><small>{choice.body}</small></span>
          </label>
        ))}
      </fieldset>
      {problem ? <Failure>{problem}</Failure> : null}
    </Dialog>
  );
}

/** The first sync, while it runs: no count, because the service does not report one. It can be dismissed; it keeps going. */
export function ConnectProgress({ onDismiss }: { onDismiss(): void }) {
  return (
    <Dialog title="Connecting your settings" onClose={onDismiss} footer={<Button tone="primary" onClick={onDismiss}>Keep working</Button>}>
      <p className="ac-wait" role="status"><Loader2 size={12} className="spin" aria-hidden />Combining settings…</p>
      <p className="ac-sub">Nothing on this computer is deleted. You can keep using Orbyn while this runs.</p>
    </Dialog>
  );
}

/** The first sync ended. */
export function ConnectDone({ synced, onDone }: { synced: number; onDone(): void }) {
  return (
    <Dialog title="Settings connected" onClose={onDone} footer={<Button tone="primary" onClick={onDone}>Done</Button>}>
      <p>{synced} {synced === 1 ? "setting" : "settings"} synced.</p>
    </Dialog>
  );
}
