import { Info, Loader2 } from "lucide-react";
import { useRef, useState, type FormEvent } from "react";
import { Button, Field } from "../../ui/controls";
import { accountApi } from "./api";
import { Card, Failure, TextLink } from "./Card";
import { emailCodeInput, inviteInput, isEmailCodeComplete, isInviteComplete, retryText } from "./codes";
import { NETWORK_TEXT, problemOf, type Problem } from "./problem";
import { useAfter, useCountdown, type Wait } from "./useCountdown";

type Step = "invite" | "details" | "password" | "code" | "done";

const RESEND_SECONDS = 60;
/** A code works for 30 minutes. The server does not say a code is old, so the app counts it on its own clock. */
const CODE_LIFE_SECONDS = 30 * 60;
/** The shortest password: nothing else about it is checked. */
export const PASSWORD_MIN = 12;
/** The password step sends before anything else does, so a network failure there says nothing was sent. */
const NOT_SENT_TEXT = "Couldn’t reach Orbyn. Check your connection and try again. Nothing was sent.";
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
/** Characters as a person counts them, so an emoji is one. */
const length = (text: string) => [...text].length;

/**
 * Create an account in five steps: the invite (while sign-up is invite-only), the email, the password with its
 * confirmation, the emailed code, and a last step that says it is done. Only the password step sends the email: it
 * creates the account and emails the code, so the earlier steps send nothing. The password stays in this component until
 * that step succeeds, goes to the service once, and is never written anywhere else.
 */
export function SignUpCard({ inviteRequired, connectNext, onSignIn, onFinished }: { inviteRequired: boolean; connectNext?: boolean; onSignIn(): void; onFinished(): void }) {
  const [step, setStep] = useState<Step>(inviteRequired ? "invite" : "details");
  const [invite, setInvite] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  // Nothing on the password step is checked until Continue is pressed.
  const [weak, setWeak] = useState(false);
  const [mismatch, setMismatch] = useState(false);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [wait, setWait] = useState<Wait | null>(null);
  const [sent, setSent] = useState(false);
  const spinning = useAfter(busy);
  const checking = useRef<AbortController | null>(null);
  // Try again after a network failure repeats whichever request failed: the code or a new code.
  const lastCall = useRef<"verify" | "resend">("verify");
  const left = useCountdown(wait);
  // A new code can be asked for once a minute, counted on the app's own clock.
  const [resendAt, setResendAt] = useState<Wait | null>(null);
  const resendLeft = useCountdown(resendAt);
  const resendWaiting = resendLeft !== null && resendLeft > 0;
  const [askedAt, setAskedAt] = useState<Wait | null>(null);
  const age = useCountdown(askedAt);
  const oldCode = age === 0;
  const throttled = left !== null && left > 0;
  // Without the invite step the later steps move up by one and there is one fewer.
  const number = { invite: 1, details: 2, password: 3, code: 4, done: 5 }[step] - (inviteRequired ? 0 : 1);
  const counted = inviteRequired ? 5 : 4;

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setProblem(null);
    try {
      await work();
    } catch (error) {
      const found = problemOf(error);
      setProblem(found);
      if (found.kind === "throttled") setWait({ seconds: found.seconds, at: Date.now() });
    } finally {
      setBusy(false);
    }
  };

  const checkInvite = (event: FormEvent) => {
    event.preventDefault();
    if (busy || !isInviteComplete(invite)) return;
    void run(async () => {
      await accountApi.checkInvite(invite);
      setStep("details");
    });
  };

  // The email step sends nothing: the password step is next.
  const toPassword = (event: FormEvent) => {
    event.preventDefault();
    if (!email.trim()) return;
    setProblem(null);
    setStep("password");
  };

  const changeEmail = () => {
    setProblem(null);
    setWeak(false);
    setMismatch(false);
    setStep("details");
  };

  const sendPassword = (event: FormEvent) => {
    event.preventDefault();
    if (busy || throttled || !password || !confirm) return;
    // Too short is said first; the confirmation is not compared until the password is long enough.
    if (length(password) < PASSWORD_MIN) {
      setWeak(true);
      return;
    }
    if (confirm !== password) {
      setMismatch(true);
      return;
    }
    const controller = new AbortController();
    checking.current = controller;
    void run(async () => {
      try {
        await accountApi.startSignUp(invite, email.trim(), password, controller.signal);
        // The account exists and the code is on its way: the password is not needed again.
        setPassword("");
        setConfirm("");
        setCode("");
        setSent(false);
        setResendAt({ seconds: RESEND_SECONDS, at: Date.now() });
        setAskedAt({ seconds: CODE_LIFE_SECONDS, at: Date.now() });
        setStep("code");
      } catch (error) {
        // Cancel returns to the form with everything still typed and nothing sent.
        if (controller.signal.aborted) return;
        throw error;
      } finally {
        checking.current = null;
      }
    });
  };

  const verify = (event: FormEvent) => {
    event.preventDefault();
    if (busy || !isEmailCodeComplete(code)) return;
    lastCall.current = "verify";
    void run(async () => {
      // The code signs the new account in: this is the last step that sends anything.
      await accountApi.verifyCode(code);
      setStep("done");
    });
  };

  const resend = () => {
    lastCall.current = "resend";
    void run(async () => {
      await accountApi.resendCode();
      setCode("");
      setResendAt({ seconds: RESEND_SECONDS, at: Date.now() });
      setAskedAt({ seconds: CODE_LIFE_SECONDS, at: Date.now() });
      setSent(true);
    });
  };

  const heading = (name: string) => <p className="ac-steps">Step {number} of {counted} · {name}</p>;
  const network = problem?.kind === "network";

  if (step === "invite") {
    return (
      <Card title="Create account" lead="Orbyn is invite-only for now" label="Create account">
        {heading("Invite code")}
        <form className="ac-form" onSubmit={checkInvite}>
          <Field
            label="Invite code"
            hint="Capitals or lowercase, with or without dashes or spaces."
            error={problem?.kind === "invite" ? "That invite code isn’t right. Check it and try again." : null}
          >
            {(props) => (
              <input {...props} className="ac-code" autoCapitalize="characters" spellCheck={false} placeholder="XXXX-XXXX-XXXX-XXXX" value={invite} disabled={busy}
                onChange={(event) => setInvite(inviteInput(event.target.value))} />
            )}
          </Field>
          {network ? <Failure>{NETWORK_TEXT}</Failure> : null}
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || !isInviteComplete(invite)}>{network ? "Try again" : "Continue"}</Button>
          </div>
        </form>
        <div className="ac-lnks">
          <span>Already have an account?</span>
          <TextLink onClick={onSignIn}>Sign in</TextLink>
        </div>
      </Card>
    );
  }

  if (step === "details") {
    return (
      <Card title="Create your account" label="Create account">
        {heading("Email")}
        {inviteRequired ? <p>Invite accepted</p> : null}
        <form className="ac-form" onSubmit={toPassword}>
          <Field label="Email" hint="Next you choose a password. Then we email a code to confirm this address.">
            {(props) => <input {...props} type="email" autoComplete="username" value={email} onChange={(event) => setEmail(event.target.value)} />}
          </Field>
          <div className="ac-acts">
            <Button type="submit" tone="primary" disabled={!email.trim()}>Continue</Button>
          </div>
        </form>
      </Card>
    );
  }

  if (step === "password" && busy && spinning) {
    return (
      <Card title="Checking this device" lead="Orbyn does a short check on this device before it sends your code." label="Create account">
        {heading("Password")}
        <p className="ac-wait" role="status"><Loader2 size={12} className="spin" aria-hidden />Checking this device…</p>
        <p className="ac-sub">This takes a moment.</p>
        <div className="ac-acts">
          <Button onClick={() => checking.current?.abort()}>Cancel</Button>
        </div>
      </Card>
    );
  }

  if (step === "password") {
    const failedCheck = problem?.kind === "check";
    const tooShort = weak && length(password) < PASSWORD_MIN;
    const retry = throttled || failedCheck || network;
    return (
      <Card
        title={throttled ? "Too many attempts" : failedCheck ? "We couldn’t finish checking this device" : "Choose a password"}
        lead={failedCheck ? "The check could not complete. Nothing was sent, and what you typed is kept." : <>For {email.trim()} · <TextLink onClick={changeEmail}>Change email</TextLink></>}
        label="Create account"
      >
        {heading("Password")}
        <form className="ac-form" onSubmit={sendPassword}>
          {throttled ? <Failure>Too many attempts. Try again {retryText(left)}.</Failure> : null}
          {network ? <Failure>{NOT_SENT_TEXT}</Failure> : null}
          {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
          <Field label="Password" hint={tooShort ? undefined : `Use at least ${PASSWORD_MIN} characters.`} error={tooShort ? `Too short. Use at least ${PASSWORD_MIN} characters.` : null}>
            {(props) => (
              <input {...props} type="password" autoComplete="new-password" value={password} disabled={busy}
                onChange={(event) => { setPassword(event.target.value); setMismatch(false); if (length(event.target.value) >= PASSWORD_MIN) setWeak(false); }} />
            )}
          </Field>
          <Field
            label="Confirm password"
            hint={throttled ? "What you typed stays in the fields. The button turns on when the time is up." : `Your password never leaves this device. Orbyn checks it without ever seeing it. We email a code to ${email.trim()} next to confirm your address.`}
            error={mismatch ? "The two passwords don’t match." : null}
          >
            {(props) => (
              <input {...props} type="password" autoComplete="new-password" value={confirm} disabled={busy}
                onChange={(event) => { setConfirm(event.target.value); setMismatch(false); }} />
            )}
          </Field>
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || throttled || !password || !confirm || tooShort}>
              {retry ? "Try again" : "Continue"}
            </Button>
            {failedCheck ? <Button onClick={() => setProblem(null)}>Cancel</Button> : null}
          </div>
        </form>
      </Card>
    );
  }

  if (step === "code") {
    const wrong = problem?.kind === "code";
    // Once the code is old and refused, asking for a new one is the main action and comes first.
    const old = wrong && oldCode;
    const retryResend = network && lastCall.current === "resend";
    const verifyButton = (
      <Button key="verify" type={retryResend ? "button" : "submit"} onClick={retryResend ? resend : undefined} tone={old ? "secondary" : "primary"} busy={busy && spinning} disabled={busy || (!retryResend && !isEmailCodeComplete(code))}>{network ? "Try again" : "Verify"}</Button>
    );
    const resendButton = (
      <Button key="resend" tone={old ? "primary" : "secondary"} onClick={resend} disabled={busy || resendWaiting}>
        {resendWaiting ? `Send a new code in ${clock(resendLeft)}` : "Send a new code"}
      </Button>
    );
    return (
      <Card title="Check your inbox" lead={`Enter the 8-character code we emailed to ${email.trim()}. It works for 30 minutes.`} label="Create account">
        {heading("Check your email")}
        <form className="ac-form" onSubmit={verify}>
          {sent ? <p role="status" className="ac-sub">We sent a new code to {email.trim()}.</p> : null}
          {network ? <Failure>{NETWORK_TEXT}</Failure> : null}
          <Field label="8-character code" error={wrong ? "That code didn’t work. Check it, or ask for a new one." : null}>
            {(props) => (
              <input {...props} className="ac-code" autoCapitalize="characters" autoComplete="one-time-code" spellCheck={false} placeholder="ABCD-EFGH" value={code} disabled={busy}
                onChange={(event) => { setSent(false); setCode(emailCodeInput(event.target.value)); }} />
            )}
          </Field>
          {wrong && oldCode ? <p className="ac-sub">Codes last 30 minutes. Ask for a new one.</p> : null}
          {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
          <div className="ac-acts">
            {wrong && oldCode ? [resendButton, verifyButton] : [verifyButton, resendButton]}
          </div>
        </form>
        <div className="ac-lnks">
          <span>Wrong address?</span>
          <TextLink onClick={changeEmail}>Change email</TextLink>
        </div>
      </Card>
    );
  }

  return (
    <Card title="Your account is ready" lead={`You’re signed in as ${email.trim()} on this computer.`} label="Create account">
      {heading("Done")}
      {connectNext ? (
        <div className="ac-note">
          <Info size={14} aria-hidden />
          <div>Next, Orbyn asks how to connect this computer’s settings to your account. You can skip it and do it later in Account.</div>
        </div>
      ) : null}
      <div className="ac-acts">
        <Button tone="primary" onClick={onFinished}>Continue</Button>
      </div>
    </Card>
  );
}
