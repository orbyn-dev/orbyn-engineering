import { Loader2, TriangleAlert } from "lucide-react";
import { useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../ui/controls";
import { ConfirmDialog, Dialog } from "../../ui/Dialog";
import { issuesApi, useTrackers } from "./api";
import { TRACKER_ADDRESS, TRACKER_NAMES, type TrackerKind, type TrackerRow, type TrackerStatus } from "./contract";
import { hostFor, knownTrackers, reasonOf, trackerDetail, trackerLine, trackerMark, trackerStatus } from "./model";
import "./trackers.css";

/**
 * Issue trackers, in Settings. A token is typed into one password field and goes outward once, in the JSON body of the connect call.
 * It is never a field the screens read: the field is uncontrolled (React puts no value attribute in the page), is emptied the
 * moment it is read, is never kept in state, a log, a URL or an error, and vanishes with its dialog. After saving, nothing
 * shows any part of it; the rows carry only a state word and the host. The keychain behind the service is the only place a
 * token is kept. GitHub needs none: it uses the person's own `gh` sign-in.
 */

const ADDRESS = new Map(Object.entries(TRACKER_ADDRESS));
const CHIPS = new Map<TrackerStatus, { label: string; tone: "ok" | "err" | "plain" }>([
  ["signed_in", { label: "Signed in", tone: "ok" }],
  ["saved", { label: "Saved", tone: "ok" }],
  ["attention", { label: "Needs attention", tone: "err" }],
  ["not_connected", { label: "Not connected", tone: "plain" }],
]);
const chipOf = (status: TrackerStatus) => CHIPS.get(status) ?? { label: "Unknown", tone: "plain" as const };

export function StatusChip({ status }: { status: TrackerStatus }) {
  const chip = chipOf(status);
  return <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone} dot`}>{chip.label}</span>;
}

/** The one hidden field, the address where the tracker needs one, and two buttons. The token is read once, on Save, and emptied at that moment. */
function TokenDialog({ tracker, replacing, address: saved, busy, error, onCancel, onSave }: {
  tracker: TrackerKind; replacing: boolean; address: string | null; busy: boolean; error: string | null; onCancel(): void; onSave(host: string | undefined, token: string): void;
}) {
  const where = ADDRESS.get(tracker);
  const field = useRef<HTMLInputElement>(null);
  const addressField = useRef<HTMLInputElement>(null);
  const [filled, setFilled] = useState(false);
  const [addressed, setAddressed] = useState(where ? (saved ?? where.initial).trim() !== "" : true);
  const name = TRACKER_NAMES[tracker];
  const save = () => {
    const input = field.current;
    if (!input || input.value === "" || busy) return;
    const typed = where ? (addressField.current?.value ?? "").trim() : undefined;
    if (where && !typed) return;
    const host = typed === undefined ? undefined : hostFor(typed);
    const token = input.value;
    // Emptied before the call: the page keeps no copy to resend, so a refused save asks for the token again.
    input.value = "";
    setFilled(false);
    onSave(host, token);
  };
  const submit = (event: FormEvent) => { event.preventDefault(); save(); };
  // Two fields have no implicit submit without a submit button, so Enter in either one saves, once.
  const enter = (event: KeyboardEvent<HTMLInputElement>) => { if (event.key === "Enter") { event.preventDefault(); save(); } };
  return (
    <Dialog title={where ? where.title : `${name} API token`} busy={busy} onClose={onCancel} className={where ? "trk-dialog-address" : undefined}
      footer={<>
        <Button disabled={busy} onClick={onCancel}>Cancel</Button>
        <Button tone="primary" busy={busy} disabled={!filled || !addressed} onClick={save}>Save token</Button>
      </>}>
      <form onSubmit={submit} autoComplete="off" data-form-type="other">
        <p>{replacing ? "Paste the new token. The saved token stays until the new one is saved." : "Paste the token. Orbyn saves it in your keychain and does not show it again."}</p>
        {where ? (
          <label className="trk-field">
            <span>{where.label}</span>
            <input ref={addressField} type="text" name="orbyn-tracker-address" defaultValue={saved ?? where.initial} maxLength={200} autoCapitalize="off" autoCorrect="off" spellCheck={false}
              autoComplete="off" data-lpignore="true" data-1p-ignore="true" data-bwignore="true" aria-describedby="trk-address-hint" disabled={busy} onKeyDown={enter}
              onChange={(event) => setAddressed(event.target.value.trim() !== "")} />
          </label>
        ) : null}
        {where ? <p id="trk-address-hint" className="setting-line-desc">{where.hint}</p> : null}
        <label className="trk-field">
          <span>API token</span>
          <input ref={field} type="password" name="orbyn-tracker-token" data-dialog-focus autoComplete="new-password" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            data-lpignore="true" data-1p-ignore="true" data-bwignore="true" data-form-type="other" aria-describedby="trk-token-hint" disabled={busy} onKeyDown={enter}
            onChange={(event) => setFilled(event.target.value !== "")} />
        </label>
        <p id="trk-token-hint" className="setting-line-desc">Hidden while you type. Never written to a file or a log.</p>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
      </form>
    </Dialog>
  );
}

type Step = "enter" | "remove";

/** The service's words for a stored connection that no longer works, above the group as the frame draws it. */
function TrackerBanner({ row, refresh }: { row: TrackerRow; refresh(): void }) {
  const name = TRACKER_NAMES[row.tracker];
  const words = trackerDetail(row) || `${name} is not working. Replace the token or remove it.`;
  return (
    <div className="set-banner err" role="alert">
      <TriangleAlert size={14} aria-hidden />
      <div>{words}</div>
      <div className="set-banner-end"><Button small tone="quiet" onClick={refresh}>Try again</Button></div>
    </div>
  );
}

function TrackerRowView({ row, replace, refresh }: { row: TrackerRow; replace(next: TrackerRow): void; refresh(): void }) {
  const name = TRACKER_NAMES[row.tracker];
  const status = trackerStatus(row);
  const [step, setStep] = useState<Step | null>(null);
  const [busy, setBusy] = useState<"saving" | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A second click while the first request is in flight is not a second write.
  const pending = useRef(false);
  const needsToken = row.tracker !== "github";
  const holds = status === "saved" || status === "attention";

  const guarded = async (phase: NonNullable<typeof busy>, write: () => Promise<TrackerRow | void>, secret?: string) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(phase);
    setError(null);
    try {
      const next = await write();
      if (next) replace(next);
      setStep(null);
    } catch (failure) {
      // The words are the service's own; one that carries the token, in any form, is replaced whole.
      setError(reasonOf(failure, secret));
    } finally {
      pending.current = false;
      setBusy(null);
    }
  };
  const save = (host: string | undefined, token: string) => guarded("saving", () => issuesApi().connect(row.tracker, host === undefined ? { token } : { host, token }), token);
  // The confirm dialog owns its own busy state and error line: a refused removal keeps it open and the token where it was.
  const remove = async () => {
    if (pending.current) return;
    pending.current = true;
    try { replace(await issuesApi().disconnect(row.tracker)); } finally { pending.current = false; }
  };
  const retry = () => { refresh(); };
  const close = () => { if (!pending.current) { setStep(null); setError(null); } };

  const idle = busy === null;
  return (
    <>
      <div role="row" className="set-tr trk-row" data-tracker={row.tracker}>
        <span role="cell"><span className="set-mark">{trackerMark(row.tracker)}</span></span>
        <span role="cell"><span className="name">{name}</span><br /><span className="sub">{trackerLine(row)}</span></span>
        <span role="cell" className="trk-chip">{busy === "saving" ? <span className="set-pill info"><Loader2 size={11} className="spin" aria-hidden />Saving</span> : <StatusChip status={status} />}</span>
        <span role="cell" className="trk-acts">
          {idle && needsToken && status === "not_connected" ? <Button small onClick={() => setStep("enter")}>Add token…</Button> : null}
          {idle && status === "attention" ? <Button small tone="quiet" onClick={retry}>Retry</Button> : null}
          {idle && holds ? <Button small tone="quiet" onClick={() => setStep("enter")}>Replace…</Button> : null}
          {idle && (status === "saved" || status === "attention") ? <Button small tone="quiet" onClick={() => setStep("remove")}>Remove…</Button> : null}
        </span>
      </div>
      {step === "enter" ? <TokenDialog tracker={row.tracker} replacing={holds} address={row.host ?? null} busy={busy === "saving"} error={error} onCancel={close} onSave={save} /> : null}
      {step === "remove" ? (
        <ConfirmDialog title={`Remove ${name}’s token from the keychain?`} tone="danger" cancelFocus onClose={close} confirmLabel="Remove token" onConfirm={remove}>
          <p>Orbyn deletes the token it saved. Issues from {name} stop showing until you add one again. Linked threads keep their links.</p>
        </ConfirmDialog>
      ) : null}
    </>
  );
}

/** The Issue trackers group of Settings › Source control. Nothing is drawn until the service reports its trackers. */
export function IssueTrackersSettings() {
  const { rows, replace, refresh } = useTrackers();
  if (rows === null) return null;
  const trackers = knownTrackers(rows);
  return (
    <section aria-labelledby="trk-head">
      {trackers.filter((row) => trackerStatus(row) === "attention").map((row) => <TrackerBanner key={row.tracker} row={row} refresh={refresh} />)}
      <div className="set-grp" id="trk-head">Issue trackers <span className="set-pill">Beta</span></div>
      <p className="setting-line-desc">GitHub uses your gh sign-in. Tokens you add for other trackers are kept in your keychain and never shown again.</p>
      <div className="set-tbl trk-table" role="table" aria-label="Issue trackers">
        {trackers.map((row) => <TrackerRowView key={row.tracker} row={row} replace={replace} refresh={refresh} />)}
      </div>
    </section>
  );
}
