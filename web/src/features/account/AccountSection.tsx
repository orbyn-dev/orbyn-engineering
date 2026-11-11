import { useEffect, useState } from "react";
import { Button } from "../../ui/controls";
import { Failure } from "./Card";
import { SignedInPage } from "./SignedInPage";
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
  const [problem, setProblem] = useState<string | null>(null);
  const view = account.data;
  // The code card stays until it is finished or given up: its own "closed" state outlives the service's pending sign-in.
  const [codeStep, setCodeStep] = useState(false);
  const asking = view?.needs_second_factor === true;
  useEffect(() => { if (asking) setCodeStep(true); }, [asking]);
  const waiting = account.status === "loading";
  const skeleton = useAfter(waiting);

  if (view && !view.available) return null;
  const reload = () => { setProblem(null); account.reload(); };
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
      : skeleton ? <div className="ac-sk" aria-hidden><i /><i /><i /></div> : null;
  } else if (asking || codeStep) {
    body = <TwoFactorCard onDone={() => { setCodeStep(false); reload(); }} onCancel={() => { setCodeStep(false); setMode("sign-in"); reload(); }} />;
  } else if (view.session && mode !== "sign-up") {
    body = (
      <>
        {account.status === "error" ? (
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
        {problem ? <Failure>{problem}</Failure> : null}
        <SignedInPage view={view} onChanged={reload} onError={setProblem} />
      </>
    );
  } else if (mode === "sign-in") {
    body = <SignInCard inviteRequired={view.invite_required} onCreate={() => setMode("sign-up")} onDone={reload} />;
  } else if (mode === "sign-up") {
    body = <SignUpCard inviteRequired={view.invite_required} onSignIn={() => setMode("sign-in")} onFinished={() => { setMode("home"); reload(); }} />;
  } else {
    body = <SignedOutHome onSignIn={() => setMode("sign-in")} onCreate={() => setMode("sign-up")} />;
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
