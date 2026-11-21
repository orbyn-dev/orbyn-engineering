import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button } from "../../ui/controls";
import { libraryApi } from "./api";
import { LIMITS } from "./contract";
import { plain, scrubValue } from "./model";

/** What each refusal means, in this page's own words. The service's text is used only for a code this page does not know, and only after the value is taken out of it. */
const OWN_TEXT = new Map<string, string>([
  ["keychain_unavailable", "The keychain is not available, so the value can’t be saved on this computer."],
  ["keychain_blocked", "The keychain is locked or the prompt was cancelled. Unlock it, then try again."],
  ["keychain_failed", "The keychain could not finish. Try again."],
  ["key_refused", "That variable name is refused or is no longer in the set."],
  ["invalid_input", `A value is 1 to ${LIMITS.value.toLocaleString("en-US")} bytes.`],
  ["desktop_required", "Saving a value needs the Orbyn desktop app."],
  ["not_found", "That set is gone. Reload the library."],
]);

/**
 * Adds or replaces one environment value (frame `workflow-env`: "Add value"). The value is a credential:
 * - the field is uncontrolled, so the text lives in the DOM only; React state holds one boolean (is there text), never the text;
 * - it is read once on submit and the field is cleared before the request leaves, so a failure does not leave it on screen;
 * - the field opts out of password managers, autofill and spellcheck, has no `name`, and the form has no action;
 * - it is never put in storage, a URL, a log or an error: the failure shown is this page's own text, and any service text is
 *   scrubbed of the value in its exact, URL-encoded, JSON-escaped and base64 forms first;
 * - the answer is `{name, present}` and nothing about the value is shown back: no length, no first or last characters.
 * One click, one write: a second Enter or click while the write is in flight does nothing.
 */
export function ValueForm({ setId, keyName, replacing, onDone, onCancel }: { setId: string; keyName: string; replacing: boolean; onDone(): void; onCancel(): void }) {
  const field = useRef<HTMLInputElement>(null);
  const pending = useRef(false);
  const [hasText, setHasText] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const submit = async () => {
    const element = field.current;
    if (pending.current || !element) return;
    const value = element.value;
    if (value === "") return;
    pending.current = true;
    setBusy(true);
    setProblem(null);
    // Cleared before the request: from here the only copy is this local, which goes out of use when the call ends.
    element.value = "";
    setHasText(false);
    try {
      await libraryApi().setKey(setId, keyName, value);
      onDone();
    } catch (error) {
      if (error instanceof ApiFailure) {
        const own = OWN_TEXT.get(error.code);
        setProblem(own ?? `Couldn’t save the value. ${plain(scrubValue(error.message, value), 160)}`);
      } else {
        setProblem("Couldn’t save the value. Something went wrong.");
      }
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return (
    <form className="lib-value" autoComplete="off" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
      <label className="lib-value-label">
        <span>{replacing ? `New value for ${plain(keyName, 60)}` : `Value for ${plain(keyName, 60)}`}</span>
        <input
          ref={field}
          type="password"
          className="input"
          autoComplete="new-password"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          maxLength={LIMITS.value}
          data-1p-ignore="true"
          data-lpignore="true"
          data-bwignore="true"
          data-form-type="other"
          data-dialog-focus
          disabled={busy}
          aria-describedby="lib-value-hint"
          onChange={(event) => setHasText(event.target.value !== "")}
        />
      </label>
      <p id="lib-value-hint" className="field-hint">Kept in this computer’s keychain. Orbyn never shows it again, and it is not saved anywhere else.</p>
      {problem ? <p className="field-error" role="alert">{problem}</p> : null}
      <div className="lib-actions">
        <Button small tone="primary" busy={busy} disabled={!hasText} onClick={() => void submit()}>Save value</Button>
        <Button small tone="quiet" disabled={busy} onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  );
}
