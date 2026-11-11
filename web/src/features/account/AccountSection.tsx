import { useEffect, useState } from "react";
import { Button } from "../../ui/controls";
import { Failure } from "./Card";
import { SignedInSection } from "./SignedInSection";
import { SignedOutHome } from "./SignedOutPage";
import { SignInCard } from "./SignInCard";
import { SignUpCard } from "./SignUpCard";
import { TwoFactorCard } from "./TwoFactorCard";
import { useAfter } from "./useCountdown";
import { useAccount } from "./useAccount";
import "./account.css";

type Mode = "home" | "sign-in" | "sign-up";

/** Settings › Account. With the gate off the category is not in the list and this shows nothing. */
export function AccountSection() {
  const account = useAccount();
  const [mode, setMode] = useState<Mode>("home");
  const [notice, setNotice] = useState<string | null>(null);
  const view = account.data;
  // The code card stays until it is finished or given up: its own "closed" state outlives the service's pending sign-in.
  const [codeStep, setCodeStep] = useState(false);
  const asking = view?.needs_second_factor === true;
  useEffect(() => { if (asking) setCodeStep(true); }, [asking]);
  const waiting = account.status === "loading";
  const skeleton = useAfter(waiting);
  // The last status is kept while a re-read fails: shown dimmed, with the sync row saying it could not check.
  const stale = account.status === "error";

  if (view && !view.available) return null;
  const reload = () => account.reload();
  let body;
  if (!view) {
    body = account.status === "error"
      ? (
        <div className="ac-loadfail" role="alert">
          <Failure>Couldn’t load your account.</Failure>
          <small>Orbyn didn’t answer. Your settings on this computer are unchanged.</small>
          <Button small onClick={reload}>Try again</Button>
        </div>
      )
      : skeleton ? <Loading /> : null;
  } else if (asking || codeStep) {
    body = <TwoFactorCard onDone={() => { setCodeStep(false); reload(); }} onCancel={(to) => { setCodeStep(false); setMode(to); reload(); }} />;
  } else if (view.session && mode !== "sign-up") {
    body = (
      <>
        {stale ? (
          <div className="ac-loadfail" role="alert">
            <Failure>Couldn’t load your account.</Failure>
            <small>Orbyn didn’t answer. Your settings on this computer are unchanged.</small>
            <Button small onClick={reload}>Try again</Button>
          </div>
        ) : null}
        {view.sync.kind === "offline" ? (
          <div className="ac-note" role="status">
            <div>
              <b>You’re offline.</b> Password, device and sign-out changes need a connection. Your settings changes stay on this computer and sync when you’re back.
            </div>
          </div>
        ) : null}
        <SignedInSection view={view} stale={stale} reload={reload} onSignedOut={(text) => { setNotice(text); setMode("home"); reload(); }} />
      </>
    );
  } else if (mode === "sign-in") {
    body = <SignInCard inviteRequired={view.invite_required} onCreate={() => setMode("sign-up")} onDone={reload} />;
  } else if (mode === "sign-up") {
    body = <SignUpCard inviteRequired={view.invite_required} onSignIn={() => setMode("sign-in")} onFinished={() => { setMode("home"); reload(); }} />;
  } else {
    body = <SignedOutHome notice={notice} inviteRequired={view.invite_required} onSignIn={() => { setNotice(null); setMode("sign-in"); }} onCreate={() => { setNotice(null); setMode("sign-up"); }} />;
  }
  return (
    <div className="account-page" aria-busy={waiting || undefined}>
      <h1 className="sr-only" tabIndex={-1} data-page-title>Account</h1>
      {view && !view.session && (mode === "sign-in" || mode === "sign-up") && !asking && !codeStep ? (
        <button type="button" className="ac-back" onClick={() => setMode("home")}>‹ Account</button>
      ) : null}
      {body}
    </div>
  );
}

/** While the account is read: the section label stays, its values are placeholders. */
function Loading() {
  return (
    <div className="ac-sk" aria-hidden>
      <h2 className="ac-grp">Sync</h2>
      <div className="ac-skr"><i /><i /></div>
      <div className="ac-skr"><i /><i /></div>
      <div className="ac-skr"><i /><i /></div>
    </div>
  );
}
