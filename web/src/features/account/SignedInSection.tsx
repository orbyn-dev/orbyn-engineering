import { useEffect, useState } from "react";
import type { AccountView } from "../../generated/AccountView";
import { accountApi } from "./api";
import { BackLink, Failure } from "./Card";
import { ConnectChoice, ConnectDone, ConnectProgress } from "./ConnectDialogs";
import { DevicesPage } from "./DevicesPage";
import { SignedInPage } from "./SignedInPage";
import { SignOutDialog, SignOutEverywhere } from "./SignOutDialogs";
import { useDevices } from "./useDevices";

type Dialog = "sign-out" | "everywhere" | null;

/** Everything shown while signed in: the Account page, its Devices page and the sign-out dialogs. */
export function SignedInSection({ view, stale, reload, onSignedOut }: { view: AccountView; stale: boolean; reload(): void; onSignedOut(notice: string): void }) {
  const [page, setPage] = useState<"account" | "devices">("account");
  const [dialog, setDialog] = useState<Dialog>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Opened by hand after "Not now"; the first time the service asks by itself.
  const [choosing, setChoosing] = useState(false);
  const [connecting, setConnecting] = useState(false);
  // The choice was made: keep its dialog closed until the account says so too, instead of flashing it back.
  const [joined, setJoined] = useState(false);
  useEffect(() => { if (view.connect === "connected") setJoined(false); }, [view.connect]);
  // While the first sync runs, read the account again until it says it is done.
  const waitingForSummary = connecting && view.connect === "connected" && !view.connect_summary;
  useEffect(() => {
    if (!waitingForSummary) return;
    const timer = setInterval(reload, 2000);
    return () => clearInterval(timer);
  }, [waitingForSummary, reload]);
  const devices = useDevices();

  if (page === "devices") {
    return (
      <>
        <BackLink onClick={() => setPage("account")} />
        <DevicesPage view={view} />
      </>
    );
  }
  return (
    <>
      {problem ? <Failure>{problem}</Failure> : null}
      <div className="ac-last" data-stale={stale || undefined}>
        <SignedInPage
          view={view}
          stale={stale}
          devices={devices.data}
          onChanged={reload}
          onError={setProblem}
          onOpenDevices={() => setPage("devices")}
          onConnect={() => setChoosing(true)}
          onSignOut={() => setDialog("sign-out")}
          onSignOutEverywhere={() => setDialog("everywhere")}
        />
      </div>
      {view.connect !== "connected" && !joined && (choosing || view.connect === "needed") ? (
        <ConnectChoice onClose={() => { setChoosing(false); reload(); }} onConnected={() => { setChoosing(false); setJoined(true); setConnecting(true); reload(); }} />
      ) : null}
      {view.connect === "connected" && connecting && !view.connect_summary ? <ConnectProgress onDismiss={() => setConnecting(false)} /> : null}
      {view.connect_summary ? (
        <ConnectDone synced={view.connect_summary.synced} onDone={() => { setConnecting(false); void accountApi.connectSeen().then(reload, reload); }} />
      ) : null}
      {dialog === "sign-out" ? (
        <SignOutDialog
          view={view}
          reload={reload}
          onClose={() => setDialog(null)}
          onDone={() => onSignedOut("Signed out. Your threads, projects and settings are still on this computer.")}
        />
      ) : null}
      {dialog === "everywhere" ? (
        <SignOutEverywhere
          view={view}
          devices={devices.data ? devices.data.length : null}
          onClose={() => setDialog(null)}
          onThisComputer={() => setDialog("sign-out")}
          onDone={() =>
            onSignedOut(
              "Signed out everywhere. Every session of your account has ended, this one too. Your threads, projects and settings are still on this computer. This device stays in your device list until someone removes it from another device.",
            )
          }
        />
      ) : null}
    </>
  );
}
