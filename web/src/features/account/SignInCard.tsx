import { useRef, useState, type FormEvent } from "react";
import { Button, Field } from "../../ui/controls";
import { accountApi } from "./api";
import { Card, Failure, TextLink } from "./Card";
import { NETWORK_TEXT, problemOf, type Problem } from "./problem";
import { retryText } from "./codes";
import { useAfter, useCountdown, type Wait } from "./useCountdown";

/** Sign in with an email and password. One message covers a wrong password and an unknown email, on purpose. */
export function SignInCard({
  inviteRequired,
  onCreate,
  onDone,
}: {
  inviteRequired: boolean;
  onCreate(): void;
  /** Signed in, or the account asks for a code next: the page reads the account again. */
  onDone(): void;
}) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [wait, setWait] = useState<Wait | null>(null);
  const passwordField = useRef<HTMLInputElement>(null);
  const spinning = useAfter(busy);
  const left = useCountdown(wait);
  const throttled = left !== null && left > 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || throttled || !email.trim() || !password) return;
    setBusy(true);
    setProblem(null);
    try {
      await accountApi.signIn(email.trim(), password);
      setPassword("");
      onDone();
    } catch (error) {
      const found = problemOf(error);
      setProblem(found);
      // The email stays; a wrong or refused password is cleared and the field gets focus.
      if (found.kind !== "network") setPassword("");
      if (found.kind === "throttled") setWait({ seconds: found.seconds, at: Date.now() });
      requestAnimationFrame(() => passwordField.current?.focus());
    } finally {
      setBusy(false);
    }
  };

  const wrong = problem?.kind === "wrong";
  return (
    <Card title="Sign in" lead="One Orbyn account for Engineering, Planner and every Orbyn app." label="Sign in">
      <form className="ac-form" onSubmit={(event) => void submit(event)}>
        <Field label="Email">
          {(props) => (
            <input {...props} type="email" autoComplete="username" value={email} disabled={busy} onChange={(event) => setEmail(event.target.value)} />
          )}
        </Field>
        <Field label="Password" error={wrong ? "That email and password don’t match. Check them and try again." : null}>
          {(props) => (
            <input {...props} ref={passwordField} type="password" autoComplete="current-password" value={password} disabled={busy} onChange={(event) => setPassword(event.target.value)} />
          )}
        </Field>
        {problem?.kind === "network" ? <Failure>{NETWORK_TEXT}</Failure> : null}
        {throttled ? <Failure>Too many attempts. Try again {retryText(left)}.</Failure> : null}
        {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
        {inviteRequired ? (
          <div className="ac-note">
            <div>
              <b>Orbyn is invite-only for now.</b> Creating an account needs an invite code. Signing in with an existing account works as usual.
            </div>
          </div>
        ) : null}
        <div className="ac-acts">
          <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || throttled || !email.trim() || !password}>
            {busy && spinning ? "Signing in…" : problem?.kind === "network" ? "Try again" : "Sign in"}
          </Button>
        </div>
      </form>
      <div className="ac-lnks">
        <span>New here?</span>
        <TextLink onClick={onCreate}>{inviteRequired ? "Create an account with an invite" : "Create an account"}</TextLink>
      </div>
    </Card>
  );
}
