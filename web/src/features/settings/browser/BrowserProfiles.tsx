import { useContext, useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";
import type { BrowserProfile } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { ConfirmDialog, Dialog } from "../../../ui/Dialog";
import { clearBrowserProfile, createBrowserProfile, deleteBrowserProfile, listBrowserProfiles, renameBrowserProfile } from "../api";
import { SettingRowIds } from "../rows/SettingRow";
import "./browserProfiles.css";

/**
 * Settings › Browser: the named profiles the service keeps. Names and the service's words are service data, drawn as
 * text and capped. Each write is one request however fast the button is pressed. Rename changes the stored name only, so a
 * profile open in a tab can be renamed.
 */

const NAME_SHOWN = 80;
const SAID_SHOWN = 200;
const cap = (text: string, limit: number) => (text.length > limit ? `${text.slice(0, limit - 1)}…` : text);

/** Only entries that are profiles: a string id and name. Anything else the service sent is left out. */
function profilesOf(data: unknown): BrowserProfile[] {
  if (!Array.isArray(data)) return [];
  return data.flatMap((entry: unknown) => {
    if (entry === null || typeof entry !== "object") return [];
    const { id, name, in_use } = entry as Record<string, unknown>;
    return typeof id === "string" && typeof name === "string" ? [{ id, name, in_use: in_use === true }] : [];
  });
}

type Step = { kind: "clear" | "delete" | "rename"; profile: BrowserProfile };

function NewProfileRow({ onDone, onCancel }: { onDone(): void; onCancel(): void }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const pending = useRef(false);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => { field.current?.focus(); }, []);
  const create = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setSaid(null);
    try {
      await createBrowserProfile(name);
      onDone();
    } catch (failure) {
      setSaid(cap(toFailure(failure).message, SAID_SHOWN));
      pending.current = false;
      setBusy(false);
    }
  };
  return (
    <div role="row" className="set-tr bp-row">
      <span role="cell" className="bp-name">
        <input ref={field} type="text" aria-label="Profile name" autoComplete="off" spellCheck={false} value={name}
          aria-invalid={said ? true : undefined} disabled={busy}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void create(); } if (event.key === "Escape") { event.stopPropagation(); if (!pending.current) onCancel(); } }} />
        {said ? <div className="field-error" role="alert">{said}</div> : null}
      </span>
      <span role="cell" className="t3">New</span>
      <span role="cell" className="bp-acts">
        <Button small tone="primary" busy={busy} onClick={() => void create()}>Create</Button>
        <Button small tone="quiet" disabled={busy} onClick={onCancel}>Cancel</Button>
      </span>
    </div>
  );
}

/** The rename dialog: one field, Cancel and Save. Enter saves; Escape cancels. Only the stored name changes. */
function RenameDialog({ profile, onClose, onRenamed }: { profile: BrowserProfile; onClose(): void; onRenamed(name: string): void }) {
  const [value, setValue] = useState(profile.name);
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const pending = useRef(false);
  const next = value.trim();
  const save = async () => {
    if (pending.current || !next) return;
    pending.current = true;
    setBusy(true);
    setSaid(null);
    try {
      const renamed = await renameBrowserProfile(profile.id, next);
      onRenamed(renamed.name);
    } catch (failure) {
      setSaid(cap(toFailure(failure).message, SAID_SHOWN));
      pending.current = false;
      setBusy(false);
    }
  };
  return (
    <Dialog title={`Rename the profile ${cap(profile.name, NAME_SHOWN)}?`} busy={busy} onClose={onClose}
      footer={<>
        <Button disabled={busy} onClick={onClose}>Cancel</Button>
        <Button tone="primary" busy={busy} disabled={!next} onClick={() => void save()}>Save</Button>
      </>}>
      <form className="rename-form" onSubmit={(event) => { event.preventDefault(); void save(); }}>
        <input className="dialog-field" aria-label="Profile name" data-dialog-focus value={value} autoComplete="off" spellCheck={false}
          disabled={busy} aria-invalid={said ? true : undefined}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => { setValue(event.target.value); setSaid(null); }} />
        {said ? <p className="field-error" role="alert">{said}</p> : null}
      </form>
    </Dialog>
  );
}

export function BrowserProfilesSection() {
  const pushToast = useUi((state) => state.pushToast);
  const read = useAsync("browser-profiles", (signal) => listBrowserProfiles(signal), { keep: true });
  const [adding, setAdding] = useState(false);
  const [step, setStep] = useState<Step | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const pending = useRef(false);
  const searchId = useContext(SettingRowIds)("Browser profiles");

  const profiles = read.status === "ready" ? profilesOf(read.data) : [];
  const renaming = step?.kind === "rename" ? step.profile : null;
  const renamed = (profile: BrowserProfile, name: string) => {
    pushToast(`Renamed the profile ${cap(profile.name, NAME_SHOWN)} to ${cap(name, NAME_SHOWN)}.`);
    setStep(null);
    read.reload();
  };
  const confirm = async ({ kind, profile }: Step) => {
    if (pending.current) return;
    pending.current = true;
    setRefused(null);
    const shown = cap(profile.name, NAME_SHOWN);
    try {
      if (kind === "clear") await clearBrowserProfile(profile.id);
      else await deleteBrowserProfile(profile.id);
      pushToast(kind === "clear" ? `Cleared the profile ${shown}.` : `Deleted the profile ${shown}.`);
      read.reload();
    } catch (failure) {
      // The service's own words, for example that a tab has the profile open. Nothing was removed.
      setRefused(cap(toFailure(failure).message, SAID_SHOWN));
    } finally {
      pending.current = false;
    }
  };

  return (
    <>
      <p className="setting-line-desc">A profile keeps cookies, storage and history between Browser tabs, so a sign-in lasts. A tab with no profile starts clean and forgets everything when it closes. A profile is open in one tab at a time.</p>
      {refused ? (
        <div className="set-banner err" role="alert"><div>{refused}</div>
          <div className="set-banner-end"><Button small onClick={() => setRefused(null)}>Dismiss</Button></div></div>
      ) : null}
      <div className="bp-head" id={searchId} tabIndex={searchId ? -1 : undefined}>
        <span className="set-grp">Profiles</span>
        {read.status === "ready" && profiles.length > 0 && !adding ? <Button small icon={<Plus size={13} aria-hidden />} onClick={() => setAdding(true)}>New profile</Button> : null}
      </div>
      {read.status === "loading" ? <Loading label="Loading browser profiles" rows={2} /> : null}
      {read.status === "error" ? (
        <div className="set-banner err" role="alert"><div>{cap(`Couldn’t read the browser profiles. ${read.error.message}`, SAID_SHOWN + 40)}</div>
          <div className="set-banner-end"><Button small onClick={read.reload}>Try again</Button></div></div>
      ) : null}
      {read.status === "ready" && profiles.length === 0 && !adding ? (
        <div className="set-empty">
          <b>No profiles yet</b>
          <span className="t3">Tabs start clean and forget everything when they close. Make a profile to keep a sign-in between tabs.</span>
          <Button icon={<Plus size={13} aria-hidden />} onClick={() => setAdding(true)}>New profile</Button>
        </div>
      ) : null}
      {read.status === "ready" && (profiles.length > 0 || adding) ? (
        <div className="set-tbl bp-table" role="table" aria-label="Browser profiles">
          <div role="row" className="set-tr h bp-row"><span role="columnheader">Profile</span><span role="columnheader">Status</span><span role="columnheader" aria-label="Actions" /></div>
          {adding ? <NewProfileRow onCancel={() => setAdding(false)} onDone={() => { setAdding(false); read.reload(); }} /> : null}
          {profiles.map((profile) => {
            const shown = cap(profile.name, NAME_SHOWN);
            return (
              <div role="row" className="set-tr bp-row" key={profile.id}>
                <span role="cell" className="bp-name"><b title={profile.name}>{shown}</b></span>
                <span role="cell">{profile.in_use ? <span className="set-pill warn dot">Open in a tab</span> : <span className="t3">Not open</span>}</span>
                <span role="cell" className="bp-acts">
                  <Button small tone="quiet" aria-label={`Rename ${shown}…`} onClick={() => setStep({ kind: "rename", profile })}>Rename…</Button>
                  <Button small tone="quiet" aria-label={`Clear ${shown}…`} onClick={() => setStep({ kind: "clear", profile })}>Clear…</Button>
                  <Button small tone="quiet" aria-label={`Delete ${shown}…`} onClick={() => setStep({ kind: "delete", profile })}>Delete…</Button>
                </span>
              </div>
            );
          })}
        </div>
      ) : null}
      {step?.kind === "clear" ? (
        <ConfirmDialog title={`Clear the profile ${cap(step.profile.name, NAME_SHOWN)}?`} tone="danger" cancelFocus confirmLabel="Clear profile" onClose={() => setStep(null)} onConfirm={() => confirm(step)}>
          <p>Its cookies, storage and history are removed, so you will be signed out of the sites it kept. The profile and its name stay. This can’t be undone.</p>
        </ConfirmDialog>
      ) : null}
      {step?.kind === "delete" ? (
        <ConfirmDialog title={`Delete the profile ${cap(step.profile.name, NAME_SHOWN)}?`} tone="danger" cancelFocus confirmLabel="Delete profile" onClose={() => setStep(null)} onConfirm={() => confirm(step)}>
          <p>The profile, its name and everything it kept are removed from this computer. Tabs can no longer pick it. This can’t be undone.</p>
        </ConfirmDialog>
      ) : null}
      {renaming ? <RenameDialog profile={renaming} onClose={() => setStep(null)} onRenamed={(name) => renamed(renaming, name)} /> : null}
    </>
  );
}
