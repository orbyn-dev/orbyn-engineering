import { useState } from "react";
import type { AccountView } from "../../generated/AccountView";
import { BackLink, Failure } from "./Card";
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
          onSignOut={() => setDialog("sign-out")}
          onSignOutEverywhere={() => setDialog("everywhere")}
        />
      </div>
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
