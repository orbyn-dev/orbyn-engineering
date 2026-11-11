import { Loader2 } from "lucide-react";
import { useRef, useState, type FormEvent } from "react";
import { Button, Field } from "../../ui/controls";
import { accountApi } from "./api";
import { Card, Failure, TextLink } from "./Card";
import { emailCodeInput, inviteInput, isEmailCodeComplete, isInviteComplete, retryText } from "./codes";
import { NETWORK_TEXT, problemOf, type Problem } from "./problem";
import { useAfter, useCountdown, type Wait } from "./useCountdown";

type Step = "invite" | "details" | "code" | "done";

/**
 * Create an account: the invite (while sign-up is invite-only), the email and password, the emailed code,
 * and a last step that confirms it. The password stays in this component until the code is verified, then
 * goes to the service once to finish; it is never written anywhere else.
 */
export function SignUpCard({ inviteRequired, onSignIn, onFinished }: { inviteRequired: boolean; onSignIn(): void; onFinished(): void }) {
  const [step, setStep] = useState<Step>(inviteRequired ? "invite" : "details");
  const [invite, setInvite] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [wait, setWait] = useState<Wait | null>(null);
  const [sent, setSent] = useState(false);
  const spinning = useAfter(busy);
  const checking = useRef<AbortController | null>(null);
  const left = useCountdown(wait);
  const throttled = left !== null && left > 0;
  const total = 4;
  const number = (inviteRequired ? 0 : 1) + { invite: 1, details: 2, code: 3, done: 4 }[step];
  const counted = inviteRequired ? total : total - 1;

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

  const sendEmail = (event: FormEvent) => {
    event.preventDefault();
    if (busy || throttled || !email.trim() || !password) return;
    const controller = new AbortController();
    checking.current = controller;
    void run(async () => {
      try {
        await accountApi.startSignUp(invite, email.trim(), controller.signal);
        setCode("");
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
    void run(async () => {
      await accountApi.verifyCode(code);
      await accountApi.register(password);
      setPassword("");
      setStep("done");
    });
  };

  const resend = () =>
    void run(async () => {
      await accountApi.resendCode();
      setCode("");
      setSent(true);
    });

  const heading = (name: string) => <p className="ac-steps">Step {number} of {counted} · {name}</p>;
  const network = problem?.kind === "network" ? <Failure>{NETWORK_TEXT}</Failure> : null;

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
          {network}
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || !isInviteComplete(invite)}>Continue</Button>
          </div>
        </form>
        <div className="ac-lnks">
          <span>Already have an account?</span>
          <TextLink onClick={onSignIn}>Sign in</TextLink>
        </div>
      </Card>
    );
  }

  if (step === "details" && busy && spinning) {
    return (
      <Card title="Checking this device" lead="Orbyn does a short check on this device before it sends your code. This takes a moment, sometimes a few seconds." label="Create account">
        {heading("Email and password")}
        <div className="ac-prog">
          <div className="ac-bar" role="progressbar" aria-label="Checking this device" />
          <small><Loader2 size={12} className="spin" aria-hidden />Checking this device…</small>
        </div>
        <div className="ac-acts">
          <Button onClick={() => checking.current?.abort()}>Cancel</Button>
        </div>
      </Card>
    );
  }

  if (step === "details") {
    const failedCheck = problem?.kind === "check";
    return (
      <Card title={throttled ? "Too many attempts" : failedCheck ? "We couldn’t finish checking this device" : "Create your account"} lead={failedCheck ? "The check could not complete. Nothing was sent, and what you typed is kept." : undefined} label="Create account">
        {heading("Email and password")}
        {inviteRequired ? <p>Invite accepted</p> : null}
        <form className="ac-form" onSubmit={sendEmail}>
          {throttled ? <Failure>Too many attempts. Try again {retryText(left)}.</Failure> : null}
          <Field label="Email">
            {(props) => <input {...props} type="email" autoComplete="username" value={email} disabled={busy} onChange={(event) => setEmail(event.target.value)} />}
          </Field>
          <Field
            label="Password"
            hint={throttled ? "The email stays in the field. The button turns on when the time is up." : "Your password never leaves this device. Orbyn checks it without ever seeing it. We email a code to confirm your address."}
          >
            {(props) => <input {...props} type="password" autoComplete="new-password" value={password} disabled={busy} onChange={(event) => setPassword(event.target.value)} />}
          </Field>
          {network}
          {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || throttled || !email.trim() || !password}>
              {throttled || failedCheck ? "Try again" : "Continue with a password"}
            </Button>
            {failedCheck ? <Button onClick={() => setProblem(null)}>Cancel</Button> : null}
          </div>
        </form>
      </Card>
    );
  }

  if (step === "code") {
    const wrong = problem?.kind === "code";
    return (
      <Card title="Check your inbox" lead={`Enter the 8-character code we emailed to ${email.trim()}. It works for 30 minutes.`} label="Create account">
        {heading("Check your email")}
        <form className="ac-form" onSubmit={verify}>
          <Field label="8-character code" error={wrong ? "That code isn’t right. Use the code in the newest email." : null}>
            {(props) => (
              <input {...props} className="ac-code" autoCapitalize="characters" autoComplete="one-time-code" spellCheck={false} placeholder="ABCD-EFGH" value={code} disabled={busy}
                onChange={(event) => { setSent(false); setCode(emailCodeInput(event.target.value)); }} />
            )}
          </Field>
          {network}
          {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || !isEmailCodeComplete(code)}>Verify</Button>
            <Button onClick={resend} disabled={busy}>Send a new code</Button>
          </div>
          {sent ? <p role="status" className="ac-sub">We sent a new code.</p> : null}
        </form>
        <div className="ac-lnks">
          <span>Wrong address?</span>
          <TextLink onClick={() => { setProblem(null); setStep("details"); }}>Change email</TextLink>
        </div>
      </Card>
    );
  }

  return (
    <Card title="Your account is ready" lead={`You’re signed in as ${email.trim()} on this computer.`} label="Create account">
      {heading("Done")}
      <div className="ac-acts">
        <Button tone="primary" onClick={onFinished}>Continue</Button>
      </div>
    </Card>
  );
}
