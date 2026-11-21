import { Copy, Pencil, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button, Pill } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { libraryApi, type LibraryData } from "./api";
import type { EnvSet, Param, Prompt, RepoEnvSet, RepoPrompt, RepoWorkflow, SuggestionFile, Workflow } from "./contract";
import { capped, keychainNote, kindLabel, plain, sourceLabel } from "./model";
import { ValueForm } from "./ValueForm";

export const MASK = "••••••••";

function ParamTable({ params }: { params: readonly Param[] }) {
  if (params.length === 0) return <p className="field-hint">No parameters.</p>;
  return (
    <table className="lib-table">
      <thead><tr><th>Name</th><th>Type</th><th>Default</th><th>Required</th></tr></thead>
      <tbody>
        {params.map((param) => (
          <tr key={param.name}>
            <td><code>{plain(param.name, 32)}</code></td>
            <td>{kindLabel(param.kind)}{param.kind === "choice" ? `: ${(param.choices ?? []).map((choice) => plain(choice, 60)).join(", ")}` : ""}</td>
            <td>{param.default ? plain(param.default, 80) : "–"}</td>
            <td>{param.required ? "Yes" : "No"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const Source = ({ repo }: { repo: boolean }) => (
  <p className="lib-source">{repo ? <>From this repository’s <code>.orbyn/</code> <Pill>Read-only</Pill> Orbyn reads it and never writes it.</> : "Only on this computer. Not in the repository."}</p>
);

/** Frame `workflow-detail`: the command (as text, with `{{name}}` marked), the parameter table, where it comes from and the actions. */
export function WorkflowPage({ workflow, repo, sets, hasThread, onUse, onEdit, onDelete, onCopy, busyCopy }: {
  workflow: Workflow | RepoWorkflow;
  repo: boolean;
  sets: readonly EnvSet[];
  hasThread: boolean;
  onUse(mode: "run" | "terminal" | "composer" | "copy"): void;
  onEdit(): void;
  onDelete(): void;
  onCopy(): void;
  busyCopy: boolean;
}) {
  const shown = capped(workflow.command, 4200);
  // A saved workflow lists sets by id; a repository entry names them (`env_set_names`).
  const attached = "env_set_ids" in workflow ? workflow.env_set_ids.map((id) => sets.find((set) => set.id === id)?.name ?? "a set that is gone") : workflow.env_set_names;
  const from = "source" in workflow ? sourceLabel(workflow.source) : null;
  return (
    <div className="lib-page">
      <h3 className="lib-h">{plain(workflow.name, 60)}{from ? <Pill>{from}</Pill> : null}</h3>
      {workflow.description ? <p className="field-hint">{plain(workflow.description, 200)}</p> : null}
      <div className="field"><span className="lib-label">Command</span><pre className="lib-pre" data-testid="command">{shown.text}{shown.cut ? "…" : ""}</pre></div>
      <div className="field"><span className="lib-label">Parameters</span><ParamTable params={workflow.params} /></div>
      {attached.length > 0 ? <p className="field-hint">Environment: {attached.map((name) => plain(name, 40)).join(", ")}. Applied by the run; values stay hidden.</p> : null}
      <Source repo={repo} />
      {"needs_ack" in workflow && workflow.needs_ack ? <p className="field-hint" role="note">This command came from outside this device. Its next run asks you to confirm first.</p> : null}
      <div className="lib-actions">
        {repo ? <Button tone="primary" busy={busyCopy} onClick={onCopy}>Copy to Personal</Button> : (
          <>
            <Button tone="primary" disabled={!hasThread} title={hasThread ? undefined : "Open a thread to run it in its checkout."} onClick={() => onUse("run")}>Run…</Button>
            <Button disabled={!hasThread} onClick={() => onUse("terminal")}>Insert into the terminal line</Button>
            <Button onClick={() => onUse("composer")}>Insert into the composer</Button>
            <Button icon={<Copy size={14} aria-hidden />} onClick={() => onUse("copy")}>Copy the command</Button>
            <Button tone="quiet" icon={<Pencil size={14} aria-hidden />} onClick={onEdit}>Edit</Button>
            <Button tone="quiet" icon={<Trash2 size={14} aria-hidden />} onClick={onDelete}>Delete</Button>
          </>
        )}
      </div>
      {repo ? <p className="field-hint">Run needs a copy in Personal first: a run starts a library item, not a file.</p> : null}
    </div>
  );
}

/** Frame `workflow-prompt`: text is not run; its main action is Insert into the composer. */
export function PromptPage({ prompt, repo, onUse, onEdit, onDelete, onCopy, busyCopy }: {
  prompt: Prompt | RepoPrompt;
  repo: boolean;
  onUse(mode: "composer" | "copy"): void;
  onEdit(): void;
  onDelete(): void;
  onCopy(): void;
  busyCopy: boolean;
}) {
  const shown = capped(prompt.body, 33_000);
  const from = "source" in prompt ? sourceLabel(prompt.source) : null;
  return (
    <div className="lib-page">
      <h3 className="lib-h">{plain(prompt.name, 60)}{from ? <Pill>{from}</Pill> : null}</h3>
      {prompt.description ? <p className="field-hint">{plain(prompt.description, 200)}</p> : null}
      <div className="field"><span className="lib-label">Prompt</span><pre className="lib-pre lib-prose" data-testid="body">{shown.text}{shown.cut ? "…" : ""}</pre></div>
      <div className="field"><span className="lib-label">Parameters</span><ParamTable params={prompt.params} /></div>
      <p className="field-hint">A prompt is text. Orbyn does not run it as a command; it goes to the engine when you send it.</p>
      <Source repo={repo} />
      <div className="lib-actions">
        {repo ? <Button tone="primary" busy={busyCopy} onClick={onCopy}>Copy to Personal</Button> : (
          <>
            <Button tone="primary" onClick={() => onUse("composer")}>Insert into the composer</Button>
            <Button icon={<Copy size={14} aria-hidden />} onClick={() => onUse("copy")}>Copy the text</Button>
            <Button tone="quiet" icon={<Pencil size={14} aria-hidden />} onClick={onEdit}>Edit</Button>
            <Button tone="quiet" icon={<Trash2 size={14} aria-hidden />} onClick={onDelete}>Delete</Button>
          </>
        )}
      </div>
    </div>
  );
}

/**
 * Frame `workflow-env`: the variables of a set, each saying whether a value is kept on this computer. A value is never shown, not even
 * masked to its length: `present` draws the same fixed mask. Add value opens the one-field form of `ValueForm`; Remove value deletes
 * the keychain item and keeps the name. Not drawn (the contract has no route): Reveal here, plain values from the repo, Apply to a terminal.
 */
export function EnvSetPage({ set, repo, onChanged, onEdit, onDelete, onCopy, busyCopy }: {
  set: EnvSet | RepoEnvSet;
  repo: boolean;
  onChanged(): void;
  onEdit(): void;
  onDelete(): void;
  onCopy(): void;
  busyCopy: boolean;
}) {
  const [adding, setAdding] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // The service’s own words when a value save answered `keychain_unavailable`: a Planned state, not an error.
  const [plannedNote, setPlannedNote] = useState<string | null>(null);
  const pending = useRef(false);
  const stored = "status" in set;
  const names = stored ? set.keys.map((key) => ({ name: key.name, present: key.present })) : set.keys.map((name) => ({ name, present: false }));
  const note = stored ? keychainNote(set) : null;

  const remove = async (key: string) => {
    if (pending.current || !stored) return;
    pending.current = true;
    setRemoving(key);
    setProblem(null);
    try {
      await libraryApi().removeKey(set.id, key);
      onChanged();
    } catch (error) {
      setProblem(error instanceof ApiFailure && error.code === "keychain_blocked" ? "The keychain is locked or the prompt was cancelled. Unlock it, then try again." : "Couldn’t remove the value. Try again.");
    } finally {
      pending.current = false;
      setRemoving(null);
    }
  };

  return (
    <div className="lib-page">
      <h3 className="lib-h">{plain(set.name, 60)}{stored && set.status === "planned" ? <Pill>Planned</Pill> : null}</h3>
      {set.description ? <p className="field-hint">{plain(set.description, 200)}</p> : null}
      <div className="field">
        <span className="lib-label">Variables · {names.length}</span>
        <ul className="lib-keys" aria-label="Variables">
          {names.map((key) => (
            <li key={key.name} className="lib-key">
              <code>{plain(key.name, 128)}</code>
              {repo ? <span className="field-hint">Name only</span> : key.present ? <span className="lib-masked" aria-label="A value is set">{MASK}</span> : <span className="field-hint">No value on this computer</span>}
              {!repo && stored && set.status === "stored" ? (
                <span className="lib-key-actions">
                  <Button small tone="quiet" disabled={adding !== null || removing !== null} onClick={() => setAdding(key.name)}>{key.present ? "Replace value" : "Add value"}</Button>
                  {key.present ? <Button small tone="quiet" busy={removing === key.name} disabled={removing !== null} onClick={() => void remove(key.name)}>Remove value</Button> : null}
                </span>
              ) : null}
            </li>
          ))}
        </ul>
        {adding !== null && stored ? <ValueForm key={adding} setId={set.id} keyName={adding} replacing={names.find((key) => key.name === adding)?.present ?? false} onDone={() => { setAdding(null); onChanged(); }} onPlanned={(message) => { setAdding(null); setPlannedNote(message); onChanged(); }} onCancel={() => setAdding(null)} /> : null}
        {problem ? <p className="field-error" role="alert">{problem}</p> : null}
      </div>
      {note ? <p className="field-hint" role="note">{note}</p> : plannedNote ? <p className="field-hint" role="note">{plannedNote}</p> : null}
      <p className="field-hint">Secrets are never in the repository or shown again. A workflow that lists this set gets them when it runs, and they are hidden in its output.</p>
      <Source repo={repo} />
      <div className="lib-actions">
        {repo ? <Button tone="primary" busy={busyCopy} onClick={onCopy}>Copy to Personal</Button> : (
          <>
            <Button tone="quiet" icon={<Pencil size={14} aria-hidden />} onClick={onEdit}>Edit names</Button>
            <Button tone="quiet" icon={<Trash2 size={14} aria-hidden />} onClick={onDelete}>Delete</Button>
          </>
        )}
      </div>
    </div>
  );
}

/** One confirm for a delete. A set also removes its keychain items; a failed removal leaves the set as it was. */
export function DeleteItem({ kind, id, name, onDone, onClose }: { kind: "workflow" | "prompt" | "env_set"; id: string; name: string; onDone(): void; onClose(): void }) {
  const pending = useRef(false);
  return (
    <ConfirmDialog
      title={`Delete ${plain(name, 40)}?`}
      confirmLabel="Delete"
      tone="danger"
      cancelFocus
      keepOpen
      onClose={onClose}
      onConfirm={async () => {
        if (pending.current) return;
        pending.current = true;
        try {
          const api = libraryApi();
          if (kind === "workflow") await api.deleteWorkflow(id); else if (kind === "prompt") await api.deletePrompt(id); else await api.deleteEnvSet(id);
          onDone();
        } catch (error) {
          if (error instanceof ApiFailure && error.code === "keychain_failed") throw new ApiFailure("keychain_failed", "The saved values could not all be removed, so nothing was deleted. Try again.", 502);
          throw error;
        } finally {
          pending.current = false;
        }
      }}
    >
      <p>{kind === "env_set" ? "This removes the set and every value kept for it on this computer. " : ""}This can’t be undone.</p>
    </ConfirmDialog>
  );
}

export type { LibraryData, SuggestionFile };
