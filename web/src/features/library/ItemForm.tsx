import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button, Field } from "../../ui/controls";
import { ConfirmDialog, Dialog } from "../../ui/Dialog";
import { Segmented } from "../../ui/Segmented";
import { libraryApi, type LibraryData } from "./api";
import { LIMITS, type EnvSet, type Kind, type Param, type ParamKind, type Prompt, type Workflow } from "./contract";
import { checkItem, checkKeyName, plain } from "./model";

type Existing = Workflow | Prompt | EnvSet;
interface Row { name: string; label: string; kind: ParamKind; default: string; required: boolean; choices: string }

const rowOf = (param: Param): Row => ({ name: param.name, label: param.label, kind: param.kind, default: param.default ?? "", required: param.required, choices: (param.choices ?? []).join(", ") });
const paramOf = (row: Row): Param => ({
  name: row.name.trim(),
  label: row.label.trim() || row.name.trim(),
  kind: row.kind,
  required: row.required,
  ...(row.default !== "" ? { default: row.default } : {}),
  ...(row.kind === "choice" ? { choices: row.choices.split(",").map((choice) => choice.trim()).filter(Boolean) } : {}),
});

const FAILURE = new Map<string, string>([
  ["library_limit", "The library is full for this kind. Delete one to add another."],
  ["desktop_required", "Saving needs the Orbyn desktop app."],
  ["key_refused", "A variable name is refused or repeated. Check the names."],
  ["not_found", "That item is gone. Reload the library."],
]);

/** The latest revision of an item, read fresh, for “Save mine” after a conflict. */
async function latestRevision(kind: Kind, id: string): Promise<number | null> {
  const all: Existing[] = kind === "workflow" ? await libraryApi().workflows() : kind === "prompt" ? await libraryApi().prompts() : await libraryApi().envSets();
  return all.find((one) => one.id === id)?.revision ?? null;
}

/**
 * One form creates and edits all three kinds (frames `workflow-edit`, `workflow-edit-error`, `workflow-conflict`). It always saves to
 * Personal, on this computer, and says so: nothing is written into a repository. Problems are marked on the field they belong to and
 * Save stays off until each is fixed; the person's text is kept as typed on every failure. A conflict (another window saved the
 * item) keeps the edits and offers Reload (discard them) or Save mine (one more confirm, then replaces the saved version).
 * No environment value is in this form: values are added one at a time on the set's own page. Not drawn: Compare (Planned), and a
 * number or yes/no parameter kind, which the contract does not have.
 */
export function ItemForm({ kind: fixed, item, library, onSaved, onReload, onClose }: {
  kind: Kind | null;
  item: Existing | null;
  library: LibraryData;
  onSaved(name: string): void;
  onReload(): void;
  onClose(): void;
}) {
  const [kind, setKind] = useState<Kind>(fixed ?? "workflow");
  const [name, setName] = useState(item?.name ?? "");
  const [description, setDescription] = useState(item?.description ?? "");
  const [text, setText] = useState(item && "command" in item ? item.command : item && "body" in item ? item.body : "");
  const [rows, setRows] = useState<Row[]>(item && "params" in item ? item.params.map(rowOf) : []);
  const [setIds, setSetIds] = useState<string[]>(item && "env_set_ids" in item ? item.env_set_ids : []);
  const [keys, setKeys] = useState(item && "keys" in item ? item.keys.map((key) => key.name).join("\n") : "");
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [taken, setTaken] = useState<string | null>(null);
  const [confirmMine, setConfirmMine] = useState(false);

  const same = kind === "workflow" ? library.workflows : kind === "prompt" ? library.prompts : library.envSets;
  const params = rows.map(paramOf);
  const base = checkItem({ name, description, text: kind === "env_set" ? "-" : text, params: kind === "env_set" ? [] : params }, same, item?.id ?? null, kind === "workflow" ? LIMITS.command : LIMITS.body);
  const names = keys.split("\n").map((line) => line.trim()).filter(Boolean);
  const keyProblem = kind !== "env_set" ? null : (() => {
    const seen: string[] = [];
    for (const one of names) { const issue = checkKeyName(one, seen); if (issue) return `${plain(one, 40)}: ${issue}`; seen.push(one); }
    return null;
  })();
  const nameError = base.name ?? (taken === name ? "That name is already used in Personal. Choose another name." : undefined);
  const invalid = Boolean(nameError || base.description || (kind !== "env_set" && (base.body || base.params)) || keyProblem || (kind === "workflow" && setIds.length > LIMITS.envSetsPerWorkflow));

  const send = async (seen: number | null) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const api = libraryApi();
      const common = { name: name.trim(), description };
      if (kind === "workflow") {
        const draft = { ...common, command: text, params, env_set_ids: setIds };
        if (item) await api.updateWorkflow(item.id, draft, seen ?? item.revision); else await api.createWorkflow(draft);
      } else if (kind === "prompt") {
        const draft = { ...common, body: text, params };
        if (item) await api.updatePrompt(item.id, draft, seen ?? item.revision); else await api.createPrompt(draft);
      } else {
        const draft = { ...common, keys: names };
        if (item) await api.updateEnvSet(item.id, draft, seen ?? item.revision); else await api.createEnvSet(draft);
      }
      onSaved(common.name);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "name_taken") setTaken(name);
      else if (error instanceof ApiFailure && error.code === "revision_conflict") setConflict(true);
      else setFailure(error instanceof ApiFailure ? (FAILURE.get(error.code) ?? `Couldn’t save. ${plain(error.message, 160)}`) : "Couldn’t save. Something went wrong.");
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const setRow = (at: number, patch: Partial<Row>) => setRows((held) => held.map((row, i) => (i === at ? { ...row, ...patch } : row)));
  return (
    <Dialog
      title={`${item ? "Edit" : "New"} ${kind === "env_set" ? "env set" : kind}`}
      busy={busy}
      onClose={onClose}
      className="lib-dialog lib-dialog-wide"
      footer={<>
        <Button onClick={onClose} disabled={busy}>Cancel</Button>
        {conflict
          ? <Button tone="primary" disabled={busy || invalid} onClick={() => setConfirmMine(true)}>Save mine</Button>
          : <Button tone="primary" busy={busy} disabled={invalid} onClick={() => void send(null)}>Save</Button>}
      </>}
    >
      <form className="lib-form" onSubmit={(event) => { event.preventDefault(); if (!invalid && !conflict) void send(null); }}>
        {conflict ? (
          <div className="lib-banner" role="alert">
            <span>This item changed since you opened it, for example in another Orbyn window. Your edits are kept here.</span>
            <Button small tone="quiet" disabled={busy} onClick={onReload}>Reload</Button>
          </div>
        ) : null}
        {item === null ? (
          <div className="field">
            <span className="lib-label">Kind</span>
            <Segmented label="Kind" value={kind} options={[{ value: "workflow", label: "Workflow" }, { value: "prompt", label: "Prompt" }, { value: "env_set", label: "Env set" }]} onChange={(value) => { setKind(value); setTaken(null); }} />
          </div>
        ) : null}
        <Field label="Name" error={nameError}>
          {(props) => <input {...props} type="text" className="input" value={name} data-dialog-focus autoComplete="off" onChange={(event) => setName(event.target.value)} />}
        </Field>
        <Field label="Description" error={base.description} hint="Optional.">
          {(props) => <input {...props} type="text" className="input" value={description} autoComplete="off" onChange={(event) => setDescription(event.target.value)} />}
        </Field>
        {kind === "env_set" ? (
          <Field label="Variable names" error={keyProblem ?? undefined} hint="One name per line. Values are added one at a time on the set’s page, never here.">
            {(props) => <textarea {...props} className="input lib-textarea" rows={4} spellCheck={false} autoComplete="off" value={keys} onChange={(event) => setKeys(event.target.value)} />}
          </Field>
        ) : (
          <>
            <Field label={kind === "workflow" ? "Command" : "Prompt"} error={base.body} hint="Write a parameter as {{name}}.">
              {(props) => <textarea {...props} className="input lib-textarea lib-mono" rows={kind === "workflow" ? 3 : 6} spellCheck={false} autoComplete="off" value={text} onChange={(event) => setText(event.target.value)} />}
            </Field>
            <div className="field">
              <span className="lib-label">Parameters</span>
              {rows.map((row, at) => (
                <div className="lib-param" key={at}>
                  <input className="input" aria-label={`Parameter ${at + 1} name`} placeholder="name" value={row.name} autoComplete="off" onChange={(event) => setRow(at, { name: event.target.value })} />
                  <input className="input" aria-label={`Parameter ${at + 1} label`} placeholder="label" value={row.label} autoComplete="off" onChange={(event) => setRow(at, { label: event.target.value })} />
                  <select className="input" aria-label={`Parameter ${at + 1} type`} value={row.kind} onChange={(event) => setRow(at, { kind: event.target.value as ParamKind })}>
                    <option value="text">Text</option><option value="choice">Choice</option><option value="path">Path</option>
                  </select>
                  {row.kind === "choice" ? <input className="input" aria-label={`Parameter ${at + 1} choices`} placeholder="a, b, c" value={row.choices} autoComplete="off" onChange={(event) => setRow(at, { choices: event.target.value })} /> : null}
                  <input className="input" aria-label={`Parameter ${at + 1} default`} placeholder="default" value={row.default} autoComplete="off" onChange={(event) => setRow(at, { default: event.target.value })} />
                  <label className="lib-check"><input type="checkbox" checked={row.required} onChange={(event) => setRow(at, { required: event.target.checked })} />Required</label>
                  <Button small tone="quiet" onClick={() => setRows((held) => held.filter((_, i) => i !== at))}>Remove parameter {at + 1}</Button>
                </div>
              ))}
              {base.params ? <p className="field-error" role="alert">{base.params}</p> : null}
              <div><Button small disabled={rows.length >= LIMITS.params} onClick={() => setRows((held) => [...held, { name: "", label: "", kind: "text", default: "", required: false, choices: "" }])}>Add parameter</Button></div>
            </div>
          </>
        )}
        {kind === "workflow" && library.envSets.length > 0 ? (
          <fieldset className="lib-sets">
            <legend className="lib-label">Environment sets (up to {LIMITS.envSetsPerWorkflow}), applied when it runs</legend>
            {library.envSets.map((set) => (
              <label className="lib-check" key={set.id}>
                <input type="checkbox" checked={setIds.includes(set.id)} onChange={(event) => setSetIds((held) => event.target.checked ? [...held, set.id] : held.filter((id) => id !== set.id))} />
                {plain(set.name, 40)}
              </label>
            ))}
            {setIds.length > LIMITS.envSetsPerWorkflow ? <p className="field-error" role="alert">Choose at most {LIMITS.envSetsPerWorkflow}.</p> : null}
          </fieldset>
        ) : null}
        <p className="field-hint">Saved to Personal, only on this computer. Orbyn writes nothing into your repository.</p>
        {failure ? <p className="field-error" role="alert">{failure}</p> : null}
      </form>
      {confirmMine ? (
        <ConfirmDialog
          title="Replace the saved version?"
          confirmLabel="Replace it"
          tone="danger"
          cancelFocus
          onClose={() => setConfirmMine(false)}
          onConfirm={async () => {
            const seen = item ? await latestRevision(kind, item.id) : null;
            if (seen === null) { setConfirmMine(false); setFailure("That item is gone. Reload the library."); return; }
            setConfirmMine(false);
            setConflict(false);
            await send(seen);
          }}
        >
          <p>Your edits replace what was saved from the other window.</p>
        </ConfirmDialog>
      ) : null}
    </Dialog>
  );
}
