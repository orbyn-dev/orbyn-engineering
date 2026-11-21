import { useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { Segmented } from "../../ui/Segmented";
import { profilesApi } from "./api";
import { DESCRIPTION_MAX, NAME_MAX, type Profile, type ProfileDraft, type ProfileWarning } from "./contract";
import { AUTOPILOT_HINTS, AUTOPILOT_IN_ORDER, autopilotLabel, checkDraft, MODE_HINTS, MODES_IN_ORDER, modeLabel, plain, TIERS_IN_ORDER, tierLabel } from "./model";

export interface EngineOption { id: string; name: string }

/**
 * Create or edit one profile (frames `profile-editor`, `profile-duplicate`). Four settings and a name: no effort, no model id, no
 * options, and no read-only mode (the service refuses one). The person's text is kept on every failure and Save stays off while a
 * field has a problem. One click, one write: a second click while the write is in flight does nothing. Not drawn: "Use for new
 * threads" (the contract has no default profile) and the engine's Sign in link; both are Planned.
 */
export function ProfileEditor({ profile, seed, others, engines, onSaved, onClose, onReload }: {
  /** The profile being edited, or `null` for a new one. */
  profile: Profile | null;
  /** The starting values: the profile's own, a built-in's for Duplicate, or empty. */
  seed: ProfileDraft;
  others: readonly Profile[];
  engines: readonly EngineOption[];
  onSaved(saved: Profile): void;
  onClose(): void;
  /** The service says the profile changed under the editor: the screen reads the list again. */
  onReload(): void;
}) {
  const [draft, setDraft] = useState<ProfileDraft>(seed);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [failure, setFailure] = useState<{ text: string; conflict: boolean } | null>(null);
  const [serverName, setServerName] = useState<{ for: string; text: string } | null>(null);
  const [warnings, setWarnings] = useState<ProfileWarning[]>([]);
  const turn = useRef(0);
  const problems = checkDraft(draft, others, profile?.id ?? null);
  const nameError = problems.name ?? (serverName && serverName.for === draft.name ? serverName.text : undefined);
  const invalid = Boolean(problems.name || problems.description || nameError);

  // The engine's answer while typing: warnings only, never blocks a save. The latest request wins; an older answer is dropped.
  useEffect(() => {
    const mine = ++turn.current;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      profilesApi().check(draft, controller.signal).then(
        (answer) => { if (mine === turn.current) setWarnings(Array.isArray(answer?.warnings) ? answer.warnings : []); },
        () => { if (mine === turn.current) setWarnings([]); },
      );
    }, 250);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [draft.engine, draft.model_tier, draft.permission_mode, draft.autopilot_mode]); // eslint-disable-line react-hooks/exhaustive-deps -- the name and description are not part of a check

  const save = async () => {
    if (pending.current || invalid) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    const body: ProfileDraft = { ...draft, name: draft.name.trim() };
    try {
      const saved = profile ? await profilesApi().update(profile.id, body, profile.revision) : await profilesApi().create(body);
      onSaved(saved);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "name_taken") setServerName({ for: draft.name, text: "A profile with that name already exists. Choose another name." });
      else if (error instanceof ApiFailure && error.code === "revision_conflict") setFailure({ text: "This profile changed since you opened it. Your edits are kept here.", conflict: true });
      else setFailure({ text: `Couldn’t save the profile. ${error instanceof ApiFailure ? plain(error.message, 200) : "Something went wrong."}`, conflict: false });
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const set = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => setDraft((held) => ({ ...held, [key]: value }));
  const engineValue = draft.engine ?? "";
  const known = engines.some((engine) => engine.id === draft.engine);
  return (
    <Dialog
      title={profile ? `Edit profile · ${plain(profile.name, 40)}` : "New profile"}
      busy={busy}
      onClose={onClose}
      className="prf-dialog"
      footer={<>
        <Button onClick={onClose} disabled={busy}>Cancel</Button>
        <Button tone="primary" busy={busy} disabled={invalid} onClick={() => void save()}>Save</Button>
      </>}
    >
      <form className="prf-form" onSubmit={(event) => { event.preventDefault(); void save(); }}>
        <Field label="Name" error={nameError}>
          {(props) => <input {...props} type="text" className="input" value={draft.name} maxLength={NAME_MAX + 20} data-dialog-focus autoComplete="off" onChange={(event) => set("name", event.target.value)} />}
        </Field>
        <Field label="Description" error={problems.description} hint="Optional.">
          {(props) => <input {...props} type="text" className="input" value={draft.description} maxLength={DESCRIPTION_MAX + 20} autoComplete="off" onChange={(event) => set("description", event.target.value)} />}
        </Field>
        <Field label="Engine" hint="Keep the thread’s engine leaves the engine alone when you apply this profile.">
          {(props) => (
            <select {...props} className="input" value={engineValue} onChange={(event) => set("engine", event.target.value === "" ? null : event.target.value)}>
              <option value="">Keep the thread’s engine</option>
              {draft.engine !== null && !known ? <option value={draft.engine}>{plain(draft.engine, 40)} (not found)</option> : null}
              {engines.map((engine) => <option key={engine.id} value={engine.id}>{plain(engine.name, 40)}</option>)}
            </select>
          )}
        </Field>
        <div className="field">
          <span className="prf-label" id="prf-tier">Model tier</span>
          <Segmented label="Model tier" value={draft.model_tier} options={TIERS_IN_ORDER.map((value) => ({ value, label: tierLabel(value) }))} onChange={(value) => set("model_tier", value)} />
          <p className="field-hint">The tier maps to a model in Engine tiers, not here.</p>
        </div>
        <div className="field">
          <span className="prf-label">Permission mode</span>
          <Segmented label="Permission mode" value={draft.permission_mode} options={MODES_IN_ORDER.map((value) => ({ value, label: modeLabel(value) }))} onChange={(value) => set("permission_mode", value)} />
          <p className="field-hint">{MODE_HINTS.get(draft.permission_mode) ?? ""}</p>
        </div>
        <div className="field">
          <span className="prf-label">Autopilot</span>
          <Segmented label="Autopilot" value={draft.autopilot_mode} options={AUTOPILOT_IN_ORDER.map((value) => ({ value, label: autopilotLabel(value) }))} onChange={(value) => set("autopilot_mode", value)} />
          <p className="field-hint">{AUTOPILOT_HINTS.get(draft.autopilot_mode) ?? ""} It applies to crews; a plain thread ignores it.</p>
        </div>
        {draft.permission_mode === "full_access" || draft.autopilot_mode === "full" ? (
          <p className="prf-note" role="note">Applying this profile asks you to confirm what it grants, every time.</p>
        ) : null}
        {warnings.map((warning) => <p className="prf-note" role="status" key={`${warning.field}:${warning.code}`}>{plain(warning.message, 160)}</p>)}
        {failure ? (
          <div className="prf-fail" role="alert">
            <span>{failure.text}</span>
            {failure.conflict ? <Button small tone="quiet" disabled={busy} onClick={onReload}>Reload</Button> : null}
          </div>
        ) : null}
      </form>
    </Dialog>
  );
}
