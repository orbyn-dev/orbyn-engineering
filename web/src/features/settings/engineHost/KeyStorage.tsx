import { Loader2 } from "lucide-react";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { Button } from "../../../ui/controls";
import { ConfirmDialog, Dialog } from "../../../ui/Dialog";
import { SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type KeyStorage, type StorageChoice, engineHostApi, useKeyStorage, plain } from "../../enginehost";
import { reasonOf } from "./HostParts";
import "./engineHost.css";

/**
 * The per-engine opt-in to keep an API key in the system keychain, behind a setting. The key is typed into one password field and
 * goes outward once, as the argument of `saveKey`. It is never a field the screens read: the field is uncontrolled (React puts no
 * value attribute in the page), is emptied the moment it is read, is never kept in state, a log or a URL, and vanishes with its
 * dialog. After saving, nothing shows any part of the key. The keychain is the only place Orbyn stores one; when it refuses,
 * Orbyn stores nothing and no file is written.
 */

type Variant = "list" | "detail";
type Step = "ask" | "enter" | "replace" | "remove";
type Chip = { label: string; tone: "ok" | "info" | "err" | "plain"; spin?: boolean };

const CHIPS = new Map<string, Chip>(Object.entries({
  not_asked: { label: "Not asked", tone: "plain" },
  saved: { label: "Saved", tone: "ok" },
  declined: { label: "Declined", tone: "plain" },
  rejected: { label: "Rejected", tone: "err" },
  saving: { label: "Saving", tone: "info", spin: true },
  removing: { label: "Removing", tone: "info", spin: true },
  locked: { label: "Locked", tone: "err" },
  denied: { label: "Denied", tone: "err" },
  remove_failed: { label: "Saved", tone: "ok" },
}));

/** A choice the service sent that this screen does not know is Unknown; it never looks like Saved. */
const chipOf = (choice: string): Chip => (typeof choice === "string" ? CHIPS.get(choice) : undefined) ?? { label: "Unknown", tone: "plain" };

function line(name: string, choice: StorageChoice, variant: Variant): string {
  switch (choice) {
    case "saved": return variant === "list" ? `Saved in your keychain. It goes only to ${name}.` : "Saved in your keychain";
    case "declined": return variant === "list" ? `Not saved. Orbyn stores nothing; ${name} keeps its own sign-in.` : `Orbyn stores nothing; ${name} keeps its own sign-in.`;
    case "saving": return "Saving it in your keychain…";
    case "removing": return "Removing it from the keychain…";
    case "locked":
    case "denied": return "Not saved. Orbyn stores nothing.";
    case "rejected": return `Saved in your keychain. ${name} rejected it.`;
    case "remove_failed": return "Still saved in your keychain";
    case "not_asked": return variant === "list" ? "Not asked yet. Orbyn stores nothing until you choose." : "Orbyn stores nothing until you choose.";
    default: return "";
  }
}

const bannerOf = (name: string, choice: StorageChoice): { text: string; retry: boolean } | null => {
  switch (choice) {
    case "locked": return { text: "The keychain is locked. Unlock it, then try again.", retry: true };
    case "denied": return { text: "The system didn’t let Orbyn use the keychain. Allow it in the system’s keychain settings, then try again.", retry: true };
    case "rejected": return { text: `${name} didn’t accept the saved key. Replace it or remove it.`, retry: false };
    case "remove_failed": return { text: "Orbyn couldn’t remove the key from the keychain. Try again.", retry: true };
    default: return null;
  }
};

function StateChip({ chip }: { chip: Chip }) {
  return (
    <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
      {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
    </span>
  );
}

const SetupPill = () => <span className="set-pill">Behind a setting</span>;

/** The one hidden field and its two buttons. The value is read once, on Save key, and the field is emptied at that moment. */
function KeyDialog({ title, lead, label, busy, error, onCancel, onSave }: {
  title: string; lead: string; label: string; busy: boolean; error: string | null; onCancel(): void; onSave(key: string): void;
}) {
  const field = useRef<HTMLInputElement>(null);
  const [filled, setFilled] = useState(false);
  const save = () => {
    const input = field.current;
    if (!input || input.value === "" || busy) return;
    const key = input.value;
    input.value = "";
    setFilled(false);
    onSave(key);
  };
  const submit = (event: FormEvent) => { event.preventDefault(); save(); };
  return (
    <Dialog title={title} busy={busy} onClose={onCancel}
      footer={<>
        <Button disabled={busy} onClick={onCancel}>Cancel</Button>
        <Button tone="primary" busy={busy} disabled={!filled} onClick={save}>Save key</Button>
      </>}>
      <form onSubmit={submit} autoComplete="off" data-form-type="other">
        <p>{lead}</p>
        <label className="eh-key">
          <span>{label}</span>
          <input ref={field} type="password" name="orbyn-api-key" data-dialog-focus autoComplete="new-password" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            data-lpignore="true" data-1p-ignore="true" data-bwignore="true" data-form-type="other" aria-describedby="eh-key-hint" disabled={busy}
            onChange={(event) => setFilled(event.target.value !== "")} />
        </label>
        <p id="eh-key-hint" className="setting-line-desc">Hidden while you type. Never written to a file or a log.</p>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
      </form>
    </Dialog>
  );
}

/** Everything one engine's key row can do: the dialogs, one write per click, and the ways the keychain refuses. */
function useEntry(row: KeyStorage, replace: (next: KeyStorage) => void) {
  const name = plain(row.name);
  const [step, setStep] = useState<Step | null>(null);
  const [busy, setBusy] = useState<"saving" | "removing" | "declining" | null>(null);
  const [failed, setFailed] = useState<{ kind: "locked" | "denied"; intent: "save" | "decline"; replacing: boolean } | "remove_failed" | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A second click while the first request is in flight is not a second write.
  const pending = useRef(false);

  const guarded = async (phase: NonNullable<typeof busy>, write: () => Promise<KeyStorage>, onFail: (failure: unknown) => void) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(phase);
    setError(null);
    try {
      replace(await write());
      setFailed(null);
      setStep(null);
    } catch (failure) {
      onFail(failure);
    } finally {
      pending.current = false;
      setBusy(null);
    }
  };

  const holdsKey = row.choice === "saved" || row.choice === "rejected" || row.choice === "remove_failed";
  const replacing = holdsKey;
  const keychainFailure = (intent: "save" | "decline") => (failure: unknown) => {
    const code = failure && typeof failure === "object" && "code" in failure ? (failure as { code: unknown }).code : null;
    if (code === KEYCHAIN_LOCKED || code === KEYCHAIN_DENIED) {
      // Nothing falls back to a file: the key is not saved, and the row says what is needed.
      setFailed({ kind: code === KEYCHAIN_LOCKED ? "locked" : "denied", intent, replacing: intent === "save" && holdsKey });
      setStep(null);
    } else setError(reasonOf(failure));
  };

  const save = (key: string) => guarded("saving", () => engineHostApi().saveKey(row.engine_id, key), keychainFailure("save"));
  const decline = () => guarded("declining", () => engineHostApi().decline(row.engine_id), keychainFailure("decline"));
  const remove = () => guarded("removing", () => engineHostApi().removeKey(row.engine_id), () => { setFailed("remove_failed"); setStep(null); });

  const choice: StorageChoice = busy === "removing" ? "removing"
    : busy === "saving" ? "saving"
    : failed === "remove_failed" ? "remove_failed"
    : failed && !failed.replacing ? failed.kind : row.choice;
  const close = () => { if (!pending.current) { setStep(null); setError(null); } };
  const retry = () => {
    if (failed === "remove_failed") { void remove(); return; }
    if (failed && failed.intent === "decline") { void decline(); return; }
    // The key was emptied when it was read, so a refused save asks for it again; no copy is kept to resend.
    setFailed(null);
    setStep(replacing ? "replace" : "enter");
  };

  const dialogs = (
    <>
      {step === "ask" ? (
        <Dialog title={`Let Orbyn keep ${name}’s API key in your keychain?`} busy={busy !== null} onClose={close}
          footer={<>
            <Button data-dialog-focus disabled={busy !== null} onClick={close}>Not now</Button>
            <Button busy={busy === "declining"} disabled={busy !== null} onClick={() => void decline()}>Don’t store</Button>
            <Button tone="primary" disabled={busy !== null} onClick={() => setStep("enter")}>Continue</Button>
          </>}>
          <p>Orbyn would keep the key in the system keychain only: never in a file, never shown again and never written to a log. It goes only to {name}. If you don’t, Orbyn stores nothing and {name} uses its own sign-in. You can remove the key at any time.</p>
          {error ? <p className="field-error" role="alert">{error}</p> : null}
        </Dialog>
      ) : null}
      {step === "enter" ? <KeyDialog title={`${name} API key`} lead="Paste the key. Orbyn saves it in your keychain and does not show it again." label="API key" busy={busy === "saving"} error={error} onCancel={close} onSave={save} /> : null}
      {step === "replace" ? <KeyDialog title={`Replace ${name}’s API key`} lead="Paste the new key. The saved key stays until the new one is saved; then Orbyn keeps only the new one." label="New API key" busy={busy === "saving"} error={error} onCancel={close} onSave={save} /> : null}
      {step === "remove" ? (
        <ConfirmDialog title={`Remove ${name}’s API key from the keychain?`} tone="danger" cancelFocus onClose={close} confirmLabel="Remove key" onConfirm={remove}>
          <p>Orbyn deletes the key it saved. {name} uses its own sign-in again, or asks you to sign in. Threads already running keep going.</p>
        </ConfirmDialog>
      ) : null}
    </>
  );

  const idle = busy === null;
  const actions = (variant: Variant): ReactNode => {
    if (!idle) return null;
    if (choice === "saved" || choice === "rejected" || choice === "remove_failed") {
      return <>
        <Button small tone="quiet" onClick={() => setStep("replace")}>Replace…</Button>
        {variant === "detail" ? <Button small tone="quiet" onClick={() => setStep("remove")}>Remove…</Button> : null}
      </>;
    }
    if (choice === "declined") return <Button small onClick={() => setStep("ask")}>Use keychain…</Button>;
    if (choice === "not_asked") return <Button small onClick={() => setStep("ask")}>Choose…</Button>;
    return null;
  };
  // A refused replace leaves the saved key where it was: the row stays Saved, the banner says so, and Remove… stays.
  const banner = failed && failed !== "remove_failed" && failed.replacing
    ? { text: "The new key wasn’t saved; the old key is still in your keychain.", retry: true }
    : bannerOf(name, choice);
  return { name, choice, actions, dialogs, banner, retry, busy: busy !== null };
}

function Banner({ text, onRetry, busy }: { text: string; onRetry: (() => void) | null; busy: boolean }) {
  return <SettingsBanner tone="err" actions={onRetry ? <Button small busy={busy} onClick={onRetry}>Try again</Button> : undefined}>{text}</SettingsBanner>;
}

function GroupRow({ row, replace }: { row: KeyStorage; replace(next: KeyStorage): void }) {
  const entry = useEntry(row, replace);
  return (
    <>
      <div role="row" className="set-tr">
        <span role="cell"><span className="set-mark">{entry.name.charAt(0).toUpperCase()}</span></span>
        <span role="cell"><span className="name">{entry.name}</span><br /><span className="sub eh-sub">{line(entry.name, entry.choice, "list")}</span></span>
        <span role="cell" className="eh-chips"><StateChip chip={chipOf(entry.choice)} /></span>
        <span role="cell">{entry.actions("list")}</span>
      </div>
      {entry.banner ? <div role="row" className="eh-row-error"><span role="cell"><Banner text={entry.banner.text} onRetry={entry.banner.retry ? entry.retry : null} busy={entry.busy} /></span></div> : null}
      {entry.dialogs}
    </>
  );
}

/** The API key storage group of the Providers page. Nothing is drawn until the service offers at least one engine. */
export function KeyStorageGroup() {
  const { rows, replace } = useKeyStorage();
  if (!rows || rows.length === 0) return null;
  return (
    <>
      <div className="set-grp">API key storage <SetupPill /></div>
      <p className="setting-line-desc eh-caption">Your choice, one engine at a time, Cursor first. Orbyn only ever uses the system keychain: never a file, never shown again, never in a log, and nothing leaves it except to the engine’s own service.</p>
      <div className="set-tbl eh-table" role="table" aria-label="API key storage">
        {rows.map((row) => <GroupRow key={row.engine_id} row={row} replace={replace} />)}
      </div>
    </>
  );
}

function DetailRow({ row, replace }: { row: KeyStorage; replace(next: KeyStorage): void }) {
  const entry = useEntry(row, replace);
  return (
    <>
      {entry.banner ? <Banner text={entry.banner.text} onRetry={entry.banner.retry ? entry.retry : null} busy={entry.busy} /> : null}
      <SettingRow title="API key" description={line(entry.name, entry.choice, "detail")}>
        <StateChip chip={chipOf(entry.choice)} />{entry.actions("detail")}
      </SettingRow>
      <p className="setting-line-desc eh-caption"><SetupPill /> Built behind a setting, off until you turn it on. Today Orbyn never stores a key.</p>
      {entry.dialogs}
    </>
  );
}

/** The API key row on one engine's detail page. Nothing when the service does not offer this engine. */
export function KeyStorageRow({ engineId }: { engineId: string }) {
  const { rows, replace } = useKeyStorage();
  const row = rows?.find((entry) => entry.engine_id === engineId);
  if (!rows || !row) return null;
  return <DetailRow row={row} replace={replace} />;
}
