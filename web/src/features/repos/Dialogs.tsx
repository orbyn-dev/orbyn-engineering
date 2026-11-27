import { useState, type ReactNode } from "react";
import { Button, Field } from "../../ui/controls";
import { canSendPersonKey } from "../../api/personKey";
import { TRUST_CONFIRM, TRUST_DECLINE, TRUST_DESKTOP_ONLY, TRUST_TITLE, TrustBody } from "../../product/ui/TrustPrompt";
import { ActionDialog } from "./ActionDialog";
import { checkCloneSource } from "../../product/git/cloneSource";
import { isFolderName, plain } from "./model";
import { reposApi } from "./api";
import { canPickFolder, pickFolder } from "./folder";
import type { RepoRow } from "./contract";

type Done = () => void;
/** The grey card of facts inside a confirm dialog, in the settings dialogs' own look. */
export const ConfirmFacts = ({ facts }: { facts: { label: string; value: ReactNode; mono?: boolean }[] }) => (
  <div className="set-dlg-card">
    {facts.map((fact) => <div key={fact.label} className="set-dlg-kv"><span>{fact.label}</span><b className={fact.mono ? "mono" : undefined}>{fact.value}</b></div>)}
  </div>
);
export const Path = ({ path }: { path: string }) => <code className="repos-path" title={plain(path)}>{plain(path)}</code>;

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

/** A new folder under the clone directory with `git init`. It starts trusted (Orbyn made it empty); `add-folder` and `clone` do not. The name is checked here by the service's rule and again there; the service picks the place. */
export function CreateDialog({ projectId, projectName, directory, onClose, onDone }: { projectId: string; projectName: string; directory: string | null; onClose(): void; onDone: Done }) {
  const [name, setName] = useState("");
  const [touched, setTouched] = useState(false);
  const ok = isFolderName(name);
  return (
    <ActionDialog title="Create a repository" confirmLabel="Create" disabled={!ok} onClose={onClose}
      onConfirm={async () => { await reposApi().create(projectId, name); onDone(); }}>
      <Field label="Folder name" error={touched && name !== "" && !ok ? "Use letters, digits, dash, underscore or dot, with no slash and no leading dot." : undefined}>
        {(field) => <input {...field} data-dialog-focus className="dialog-field" value={name} placeholder="orbyn-docs" spellCheck={false}
          onChange={(event) => setName(event.target.value)} onBlur={() => setTouched(true)} />}
      </Field>
      <p className="field-hint">
        {directory ? <>A new folder is made in <Path path={directory} />, set up with Git, and joins the project <b>{plain(projectName)}</b>.</> : <>A new folder is made and set up with Git, and joins the project <b>{plain(projectName)}</b>.</>}
        {" "}It starts trusted, because Orbyn made it empty.
      </p>
    </ActionDialog>
  );
}

/** Stops a clone that is running. The service removes the partial folder and keeps the repository listed as failed ("Cancelled.") until it is removed. */
export function CancelCloneDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={`Cancel cloning ${plain(repo.name)}?`} confirmLabel="Cancel clone" cancelLabel="Keep cloning" tone="danger" cancelFocus onClose={onClose}
      onConfirm={async () => { await reposApi().cancelClone(projectId, repo.id); onDone(); }}>
      <p>The clone stops and its partial folder is removed. The repository stays listed as failed until you remove it from the project.</p>
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
      <Field label="Git URL" error={touched && !checked.ok && text.trim() !== "" ? checked.reason : undefined}>
        {(field) => <input {...field} data-dialog-focus className="dialog-field" value={text} placeholder="https://github.com/orbyn-dev/orbyn-docs.git" spellCheck={false}
          onChange={(event) => setText(event.target.value)} onBlur={() => setTouched(true)} />}
      </Field>
      <p className="field-hint">
        {directory ? <>It is cloned into <Path path={directory} /> and joins the project <b>{plain(projectName)}</b>.</> : <>It joins the project <b>{plain(projectName)}</b>.</>}
        {" "}It runs in the background and is not trusted until you trust it.
      </p>
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

/**
 * Trusts this one folder and no other. Opening it runs nothing. The prompt is the app's own trust prompt (its title, warning,
 * path and button words come from product/ui/TrustPrompt), shown as a dialog over the project's settings: no second copy of it lives here.
 * Trusting is a `Person` route, so a page with no person key (a browser tab) says why and keeps only the way out.
 */
export function TrustDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={TRUST_TITLE(plain(repo.name))} confirmLabel={TRUST_CONFIRM} cancelLabel={TRUST_DECLINE} cancelFocus confirmHidden={!canSendPersonKey()} onClose={onClose}
      onConfirm={async () => { await reposApi().trust(projectId, repo.id); onDone(); }}>
      <TrustBody path={plain(repo.path)} />
      {canSendPersonKey() ? null : <p>{TRUST_DESKTOP_ONLY}</p>}
      <p className="field-hint">Trusting this folder does not trust the project's other repositories.</p>
    </ActionDialog>
  );
}

/**
 * Confirms one `git init` in a folder: for a repository row and for the project's own folder, since both are the same
 * write with the same meaning. `setup` is the route that writes, so this dialog never picks between them.
 */
export function SetupGitDialog({ name, path, setup, onClose, onDone }: { name: string; path: string; setup(): Promise<unknown>; onClose(): void; onDone: Done }) {
  return (
    <ActionDialog title={`Set up Git in ${plain(name)}?`} confirmLabel="Set up Git" cancelFocus onClose={onClose}
      onConfirm={async () => { await setup(); onDone(); }}>
      <p>This runs <code>git init</code> in the folder. Existing files are not changed.</p>
      <ConfirmFacts facts={[{ label: "Folder", value: <Path path={path} />, mono: true }]} />
    </ActionDialog>
  );
}

