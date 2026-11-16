import { useState, type FormEvent } from "react";
import { Button, Field } from "../../ui/controls";
import { accountApi } from "./api";
import { Card, Failure } from "./Card";
import {
  authenticatorInput,
  isAuthenticatorComplete,
  isRecoveryComplete,
  recoveryInput,
} from "./codes";
import { NETWORK_TEXT, problemOf, type Problem } from "./problem";
import { useAfter } from "./useCountdown";

type Mode = "authenticator" | "recovery";

/** The second step of signing in: a six-digit authenticator code, or a recovery code. Five wrong ones close the sign-in. */
export function TwoFactorCard({ onDone, onCancel }: { onDone(): void; onCancel(to: "sign-in" | "home"): void }) {
  const [mode, setMode] = useState<Mode>("authenticator");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const spinning = useAfter(busy);
  const recovery = mode === "recovery";
  const complete = recovery ? isRecoveryComplete(code) : isAuthenticatorComplete(code);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || !complete) return;
    setBusy(true);
    setProblem(null);
    try {
      await accountApi.secondFactor(code);
      onDone();
    } catch (error) {
      setProblem(problemOf(error));
    } finally {
      setBusy(false);
    }
  };

  // Both end the pending sign-in; they differ in where the person lands.
  const leave = (to: "sign-in" | "home") => {
    void accountApi.cancelSignIn().catch(() => undefined).finally(() => onCancel(to));
  };

  if (problem?.kind === "closed") {
    return (
      <Card title="Sign-in closed" label="Sign in · two-factor">
        <Failure>Five codes in a row didn’t work, so this sign-in was closed. Nothing was changed on your account.</Failure>
        <p>Start again with your email and password. If you can’t use your authenticator, use one of your recovery codes.</p>
        <div className="ac-acts">
          <Button tone="primary" onClick={() => leave("sign-in")}>Start again</Button>
          <Button onClick={() => leave("home")}>Back to Account</Button>
        </div>
      </Card>
    );
  }

  const wrong = problem?.kind === "code";
  const error = wrong
    ? recovery
      ? "That recovery code didn’t work. Check it, or use a code you haven’t used."
      : `That code isn’t right. Codes change every 30 seconds.${problem.triesLeft === null ? "" : ` ${problem.triesLeft} ${problem.triesLeft === 1 ? "try" : "tries"} left.`}`
    : null;
  return (
    <Card
      title={recovery ? "Enter a recovery code" : "Enter your two-factor code"}
      lead={
        recovery
          ? "Use one of the ten recovery codes you saved when you set up two-factor. Each code works once."
          : "Open your authenticator app and enter the 6-digit code for Orbyn."
      }
      label="Sign in · two-factor"
    >
      <form className="ac-form" onSubmit={(event) => void submit(event)}>
        <Field label={recovery ? "Recovery code" : "6-digit code"} error={error}>
          {(props) => (
            <input
              {...props}
              className="ac-code"
              inputMode={recovery ? "text" : "numeric"}
              autoComplete="one-time-code"
              autoCapitalize="characters"
              spellCheck={false}
              value={code}
              disabled={busy}
              placeholder={recovery ? "XXXXX-XXXXX" : "000 000"}
              data-dialog-focus
              onChange={(event) => setCode(recovery ? recoveryInput(event.target.value) : authenticatorInput(event.target.value))}
            />
          )}
        </Field>
        {problem?.kind === "network" ? <Failure>{NETWORK_TEXT}</Failure> : null}
        <div className="ac-acts">
          <Button type="submit" tone="primary" busy={busy && spinning} disabled={busy || !complete}>Verify</Button>
          <Button onClick={() => leave("sign-in")} disabled={busy}>Cancel</Button>
        </div>
      </form>
      <div className="ac-lnks">
        <button
          type="button"
          className="ac-lnk"
          onClick={() => {
            setMode(recovery ? "authenticator" : "recovery");
            setCode("");
            setProblem(null);
          }}
        >
          {recovery ? "Use an authenticator code instead" : "Use a recovery code"}
        </button>
      </div>
    </Card>
  );
}
