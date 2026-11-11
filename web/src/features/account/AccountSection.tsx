import { useEffect, useState } from "react";
import { Button } from "../../ui/controls";
import { accountApi } from "./api";
import { clearConnectRequest } from "./changes";
import { Dialog } from "../../ui/Dialog";
import { BackLink, Failure } from "./Card";
import { ApprovalCard, ClosedCard } from "./ApprovalCard";
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
  // An ask to connect settings is only for a signed-in page: once the account is read without a session, it is dropped.
  const signedOut = view !== null && view !== undefined && !view.session;
  useEffect(() => { if (signedOut) clearConnectRequest(); }, [signedOut]);
  // The person tapped They don’t match on this screen: it closes like any other request, with the same words.
  const [mismatch, setMismatch] = useState(false);
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
  } else if (mismatch && !view.session) {
    body = <ClosedCard onLeave={(to) => { setMismatch(false); setMode(to); reload(); }} />;
  } else if (view.approval && !view.session) {
    body = <ApprovalCard approval={view.approval} onChanged={reload} onBack={(to) => { setMode(to); reload(); }} onMismatch={() => { setMismatch(true); reload(); }} />;
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
        <SignedInSection view={view} stale={stale} reload={reload} onSignedOut={(text) => { clearConnectRequest(); setNotice(text); setMode("home"); reload(); }} />
      </>
    );
  } else if (mode === "sign-in") {
    body = <SignInCard inviteRequired={view.invite_required} onCreate={() => setMode("sign-up")} onDone={reload} />;
  } else if (mode === "sign-up") {
    body = <SignUpCard inviteRequired={view.invite_required} onSignIn={() => setMode("sign-in")} onFinished={() => { setMode("home"); reload(); }} />;
  } else {
    body = <SignedOutHome notice={notice} inviteRequired={view.invite_required} onSignIn={() => { setNotice(null); setMode("sign-in"); }} onCreate={() => { setNotice(null); setMode("sign-up"); }} />;
  }
  // The service ended this computer's session and the person did not; it says so until it is dismissed once.
  const ended = view?.ended_notice === true && !view.session;
  const dismissEnded = () => void accountApi.endedSeen().then(reload, reload);
  return (
    <div className="account-page" aria-busy={waiting || undefined}>
      {ended ? (
        <Dialog
          title="You were signed out"
          onClose={dismissEnded}
          footer={
            <>
              <Button onClick={dismissEnded}>Not now</Button>
              <Button tone="primary" onClick={() => { dismissEnded(); setMode("sign-in"); }}>Sign in again</Button>
            </>
          }
        >
          <p>This device may have been removed, or your sessions ended.</p>
          <p className="ac-sub">Your threads, projects and settings are still on this computer. Orbyn keeps working without an account. Sign in again to sync your settings.</p>
        </Dialog>
      ) : null}
      <h1 className="sr-only" tabIndex={-1} data-page-title>Account</h1>
      {view && !view.session && (mode === "sign-in" || mode === "sign-up") && !asking && !codeStep && !view.approval ? (
        <BackLink onClick={() => setMode("home")} />
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
