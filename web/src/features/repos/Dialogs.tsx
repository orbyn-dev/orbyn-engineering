import { useId, useState, type ReactNode } from "react";
import { Button, Field } from "../../ui/controls";
import { ActionDialog } from "./ActionDialog";
import { checkCloneSource, isPublishName, plain } from "./model";
import { reposApi } from "./api";
import { canPickFolder, pickFolder } from "./folder";
import type { RepoRow } from "./contract";

type Done = () => void;
/** The grey card of facts inside a confirm dialog, in the settings dialogs' own look. */
const ConfirmFacts = ({ facts }: { facts: { label: string; value: ReactNode; mono?: boolean }[] }) => (
  <div className="set-dlg-card">
    {facts.map((fact) => <div key={fact.label} className="set-dlg-kv"><span>{fact.label}</span><b className={fact.mono ? "mono" : undefined}>{fact.value}</b></div>)}
  </div>
);
const Path = ({ path }: { path: string }) => <code className="repos-path" title={plain(path)}>{plain(path)}</code>;

/**
 * Adds a folder in place. The path is the person's text and goes to the service as typed. In the desktop app Browse… fills the
 * field from the native folder dialog; what it picks is text to read and change, never added or trusted by the pick itself.
 */
export function AddFolderDialog({ projectId, onClose, onDone }: { projectId: string; onClose(): void; onDone: Done }) {
  const [path, setPath] = useState("");
  const [picking, setPicking] = useState(false);
  const [pickError, setPickError] = useState<string | null>(null);
  const typed = path.trim();
  const browse = async () => {
    if (picking) return;
    setPicking(true);
    setPickError(null);
    try {
      const chosen = await pickFolder();
      if (chosen !== null) setPath(chosen);
    } catch (failure) {
      setPickError(failure instanceof Error ? failure.message : "The folder dialog did not open. Type the path instead.");
    } finally {
      setPicking(false);
    }
  };
  return (
    <ActionDialog title="Add a folder" confirmLabel="Add" disabled={typed === "" || picking} onClose={onClose}
      onConfirm={async () => { await reposApi().addFolder(projectId, typed); onDone(); }}>
      <Field label="Folder on this computer" error={pickError ?? undefined}>
        {(field) => (
          <span className="repos-folder-field">
            <input {...field} data-dialog-focus className="dialog-field" value={path} placeholder="~/Documents/Github/orbyn-docs" spellCheck={false}
              onChange={(event) => setPath(event.target.value)} />
            {canPickFolder() ? <Button type="button" busy={picking} onClick={() => void browse()}>Browse…</Button> : null}
          </span>
        )}
      </Field>
      <p className="field-hint">The folder is added in place. Nothing is copied. It is not trusted until you trust it.</p>
    </ActionDialog>
  );
}

/** The clone dialog of Add a project, with one added line saying which project the clone joins. */
export function CloneDialog({ projectId, projectName, directory, onClose, onDone }: { projectId: string; projectName: string; directory: string | null; onClose(): void; onDone: Done }) {
  const [text, setText] = useState("");
  const [touched, setTouched] = useState(false);
  const checked = checkCloneSource(text);
  return (
    <ActionDialog title="Clone a repository" confirmLabel="Clone" disabled={!checked.ok} onClose={onClose}
      onConfirm={async () => { if (checked.ok) { await reposApi().clone(projectId, checked.source); onDone(); } }}>
      <Field label="Git URL or owner/name" error={touched && !checked.ok && text.trim() !== "" ? checked.reason : undefined}>
        {(field) => <input {...field} data-dialog-focus className="dialog-field" value={text} placeholder="orbyn-dev/orbyn-docs" spellCheck={false}
          onChange={(event) => setText(event.target.value)} onBlur={() => setTouched(true)} />}
      </Field>
      <p className="field-hint">
        {directory ? <>It is cloned into <Path path={directory} /> and joins the project <b>{plain(projectName)}</b>.</> : <>It joins the project <b>{plain(projectName)}</b>.</>}
        {" "}It runs in the background and is not trusted until you trust it.
      </p>
    </ActionDialog>
  );
}

/** Publishing creates the repository on the host and pushes the current branch: the dialog names both. */
export function PublishDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  const [name, setName] = useState(`orbyn-dev/${repo.name}`);
  const [visibility, setVisibility] = useState<"private" | "public">("private");
  const group = useId();
  return (
    <ActionDialog title={`Publish ${plain(repo.name)}?`} confirmLabel="Publish" disabled={!isPublishName(name)} onClose={onClose}
      onConfirm={async () => { await reposApi().publish(projectId, repo.id, { name: name.trim(), visibility }); onDone(); }}>
      <Field label="Name" error={name !== "" && !isPublishName(name) ? "Use owner/name." : undefined}>
        {(field) => <input {...field} data-dialog-focus className="dialog-field" value={name} spellCheck={false} onChange={(event) => setName(event.target.value)} />}
      </Field>
      <fieldset className="repos-visibility" aria-labelledby={group}>
        <legend id={group}>Who can see it</legend>
        {(["private", "public"] as const).map((value) => (
          <label key={value}><input type="radio" name={group} checked={visibility === value} onChange={() => setVisibility(value)} />{value === "private" ? "Private" : "Public"}</label>
        ))}
      </fieldset>
      <ConfirmFacts facts={[{ label: "Folder", value: <Path path={repo.path} />, mono: true }, { label: "Branch pushed", value: plain(repo.branch ?? "Unknown"), mono: true }]} />
    </ActionDialog>
  );
}

export function RemoveDialog({ projectId, projectName, repo, onClose, onDone }: { projectId: string; projectName: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={`Remove ${plain(repo.name)} from ${plain(projectName)}?`} confirmLabel="Remove" tone="danger" cancelFocus onClose={onClose}
      onConfirm={async () => { await reposApi().remove(projectId, repo.id); onDone(); }}>
      <p>Orbyn stops tracking it in this project. Files on disk stay, and threads keep their history.</p>
      <ConfirmFacts facts={[{ label: "Folder", value: <Path path={repo.path} />, mono: true }]} />
    </ActionDialog>
  );
}

/** Trusts this one folder and no other. Opening it runs nothing. */
export function TrustDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={`Trust ${plain(repo.name)}?`} confirmLabel="Trust folder" cancelLabel="Don’t trust" cancelFocus onClose={onClose}
      onConfirm={async () => { await reposApi().trust(projectId, repo.id); onDone(); }}>
      <p>Agents can read files, run commands and edit code in this folder. Trust folders you know.</p>
      <p className="field-hint">Trusting this folder does not trust the project's other repositories.</p>
      <ConfirmFacts facts={[{ label: "Path", value: <Path path={repo.path} />, mono: true }]} />
    </ActionDialog>
  );
}

export function SetupGitDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={`Set up Git in ${plain(repo.name)}?`} confirmLabel="Set up Git" cancelFocus onClose={onClose}
      onConfirm={async () => { await reposApi().setupGit(projectId, repo.id); onDone(); }}>
      <p>This runs <code>git init</code> in the folder. Existing files are not changed.</p>
      <ConfirmFacts facts={[{ label: "Folder", value: <Path path={repo.path} />, mono: true }]} />
    </ActionDialog>
  );
}

