import { Loader2 } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import { Button } from "../../../ui/controls";
import { ConfirmDialog, Dialog } from "../../../ui/Dialog";
import { SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type SignInStorage, type StorageChoice } from "./contract";
import { engineHostApi, useSignInStorage } from "./hostApi";
import { shortDate } from "./hostText";
import { reasonOf } from "./HostParts";
import "./engineHost.css";

/**
 * The per-engine opt-in to keep a sign-in in the system keychain. This file handles a choice and a count of
 * threads, never a credential: the contract has no field that could hold one, and nothing here reads, logs or
 * puts a value into a URL. The keychain is the only place Orbyn stores a sign-in, never a file, and when it
 * refuses, Orbyn stores nothing.
 */

type Variant = "list" | "detail";
type Intent = "allow" | "decline";

interface Shown { chip: { label: string; tone: "ok" | "info" | "err" | "plain"; spin?: boolean }; line: string }

function shown(row: SignInStorage, choice: StorageChoice, variant: Variant): Shown {
  switch (choice) {
    case "allowed": return { chip: { label: "Keychain on", tone: "ok" }, line: variant === "list"
      ? `Allowed. Its sign-in is kept in the keychain and goes only to ${row.name}.`
      : `Allowed ${shortDate(row.allowed_at)}. Kept in the keychain; goes only to ${row.name}.`.replace("Allowed .", "Allowed.") };
    case "declined": return { chip: { label: "Declined", tone: "plain" }, line: `Declined. Orbyn stores nothing; ${row.name} keeps its own sign-in.` };
    case "removing": return { chip: { label: "Removing", tone: "info", spin: true }, line: "Removing it from the keychain…" };
    case "locked": return { chip: { label: "Locked", tone: "err" }, line: `Not saved. ${row.name} needs you to sign in again.` };
    case "denied": return { chip: { label: "Denied", tone: "err" }, line: "Not saved. Orbyn stores nothing." };
    default: return { chip: { label: "Not asked", tone: "plain" }, line: "Not asked yet. Orbyn stores nothing until you choose." };
  }
}

const BANNER: Record<"locked" | "denied", string> = {
  locked: "The keychain is locked. Unlock it, then try again.",
  denied: "The system didn’t let Orbyn use the keychain. Allow it in the system’s keychain settings, then try again.",
};

function Chip({ chip }: { chip: Shown["chip"] }) {
  return (
    <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
      {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
    </span>
  );
}

const threads = (count: number) => `${count} ${count === 1 ? "thread" : "threads"}`;

/** Everything one engine's storage choice can do: the dialogs, the one write per click, and the two ways the keychain refuses. */
function useEntry(row: SignInStorage, replace: (next: SignInStorage) => void) {
  const [asking, setAsking] = useState(false);
  const [revoking, setRevoking] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [failed, setFailed] = useState<{ kind: "locked" | "denied"; intent: Intent } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A second click while the first request is in flight is not a second write.
  const pending = useRef(false);

  const choose = async (intent: Intent) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      replace(await engineHostApi().choose(row.engine_id, intent));
      setFailed(null);
      setAsking(false);
    } catch (failure) {
      const code = failure && typeof failure === "object" && "code" in failure ? (failure as { code: unknown }).code : null;
      if (code === KEYCHAIN_LOCKED || code === KEYCHAIN_DENIED) {
        // Nothing falls back to a file: the choice is not saved, and the row says what is needed.
        setFailed({ kind: code === KEYCHAIN_LOCKED ? "locked" : "denied", intent });
        setAsking(false);
      } else setError(reasonOf(failure));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const revoke = async () => {
    if (pending.current) return;
    pending.current = true;
    setRemoving(true);
    try { replace(await engineHostApi().revoke(row.engine_id)); setFailed(null); }
    finally { pending.current = false; setRemoving(false); }
  };

  const choice: StorageChoice = removing ? "removing" : failed ? failed.kind : row.choice;
  const retry = choice === "locked" || choice === "denied" ? () => void choose(failed?.intent ?? "allow") : null;
  const dialogs = (
    <>
      {asking ? (
        <Dialog title={`Keep ${row.name}’s sign-in in your keychain?`} busy={busy} onClose={() => setAsking(false)}
          footer={<>
            <Button data-dialog-focus disabled={busy} onClick={() => setAsking(false)}>Not now</Button>
            <Button disabled={busy} onClick={() => void choose("decline")}>Don’t store</Button>
            <Button tone="primary" busy={busy} onClick={() => void choose("allow")}>Use keychain</Button>
          </>}>
          <p>Orbyn would store it in the system keychain, never in a file, and send it only to {row.name}. If you don’t, Orbyn stores nothing and {row.name} keeps its own sign-in. You can change this at any time.</p>
          {error ? <p className="field-error" role="alert">{error}</p> : null}
        </Dialog>
      ) : null}
      {revoking ? (
        <ConfirmDialog title={`Remove ${row.name}’s sign-in from the keychain?`} tone="danger" cancelFocus onClose={() => setRevoking(false)}
          confirmLabel={row.running_threads > 0 ? `Stop ${threads(row.running_threads)} and remove` : "Remove"}
          onConfirm={revoke}>
          <p>{`Orbyn deletes the item it stored. ${row.running_threads > 0
            ? `${threads(row.running_threads)} ${row.running_threads === 1 ? "is" : "are"} running on ${row.name}; ${row.running_threads === 1 ? "it stops" : "they stop"} now and can be resumed after you sign in again. `
            : ""}Orbyn stores nothing after this.`}</p>
        </ConfirmDialog>
      ) : null}
    </>
  );
  /** The one action that fits the choice. A refused keychain offers Try again in its banner instead. */
  const action: ReactNode = choice === "allowed"
    ? <Button small onClick={() => setRevoking(true)}>Revoke…</Button>
    : choice === "declined"
      ? <Button small onClick={() => setAsking(true)}>Use keychain…</Button>
      : choice === "not_asked" ? <Button small onClick={() => setAsking(true)}>Choose…</Button> : null;
  return { choice, action, dialogs, retry, banner: choice === "locked" || choice === "denied" ? BANNER[choice] : null, busy };
}

function Banner({ text, onRetry, busy }: { text: string; onRetry(): void; busy: boolean }) {
  return <SettingsBanner tone="err" actions={<Button small busy={busy} onClick={onRetry}>Try again</Button>}>{text}</SettingsBanner>;
}

function GroupRow({ row, replace }: { row: SignInStorage; replace(next: SignInStorage): void }) {
  const entry = useEntry(row, replace);
  const view = shown(row, entry.choice, "list");
  return (
    <>
      <div role="row" className="set-tr">
        <span role="cell"><span className="set-mark">{row.name.charAt(0).toUpperCase()}</span></span>
        <span role="cell"><span className="name">{row.name}</span><br /><span className="sub eh-sub">{view.line}</span></span>
        <span role="cell" className="eh-chips"><Chip chip={view.chip} /></span>
        <span role="cell">{entry.action}</span>
      </div>
      {entry.banner && entry.retry ? <div role="row" className="eh-row-error"><span role="cell"><Banner text={entry.banner} onRetry={entry.retry} busy={entry.busy} /></span></div> : null}
      {entry.dialogs}
    </>
  );
}

/** The Sign-in storage group of the Providers page. Nothing is drawn until the service offers at least one engine. */
export function SignInStorageGroup() {
  const { rows, set } = useSignInStorage();
  const latest = useRef<SignInStorage[] | null>(null);
  latest.current = rows;
  if (!rows || rows.length === 0) return null;
  // `replace` runs after an await: it reads the rows as they are then, not as they were at the click, so a slower
  // answer for one engine cannot put an older copy of another engine's row back.
  const replace = (next: SignInStorage) => set((latest.current ?? rows).map((row) => (row.engine_id === next.engine_id ? next : row)));
  return (
    <>
      <div className="set-grp">Sign-in storage</div>
      <p className="setting-line-desc eh-caption">Your choice, one engine at a time. Orbyn only ever uses the system keychain: never a file, and nothing leaves it except to the engine’s own service.</p>
      <div className="set-tbl eh-table" role="table" aria-label="Sign-in storage">
        {rows.map((row) => <GroupRow key={row.engine_id} row={row} replace={replace} />)}
      </div>
    </>
  );
}

function DetailRow({ row, replace }: { row: SignInStorage; replace(next: SignInStorage): void }) {
  const entry = useEntry(row, replace);
  const view = shown(row, entry.choice, "detail");
  return (
    <>
      {entry.banner && entry.retry ? <Banner text={entry.banner} onRetry={entry.retry} busy={entry.busy} /> : null}
      <SettingRow title="Sign-in storage" description={view.line}>
        <Chip chip={view.chip} />{entry.action}
      </SettingRow>
      {entry.dialogs}
    </>
  );
}

/** The Sign-in storage row on one engine's detail page. Nothing when the service does not offer this engine. */
export function SignInStorageRow({ engineId }: { engineId: string }) {
  const { rows, set } = useSignInStorage();
  const latest = useRef<SignInStorage[] | null>(null);
  latest.current = rows;
  const row = rows?.find((entry) => entry.engine_id === engineId);
  if (!rows || !row) return null;
  const replace = (next: SignInStorage) => set((latest.current ?? rows).map((entry) => (entry.engine_id === next.engine_id ? next : entry)));
  return <DetailRow row={row} replace={replace} />;
}
