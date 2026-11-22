import { useEffect, useRef, useState } from "react";
import { toFailure } from "../../lib/useAsync";
import { Button, Field, Pill } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { reposApi } from "./api";
import type { RepoRow, RepoScan, ScannedFolder } from "./contract";
import { TrustDialog, Path } from "./Dialogs";
import { canPickFolder, pickFolder } from "./folder";
import { plain } from "./model";
import { canAddScanned, REFUSED_TEXT } from "./scanModel";

export type ScanMode = "folder" | "workspace";

const COPY = {
  folder: {
    title: "Scan a folder", label: "Folder on this computer", placeholder: "~/Documents/Github", run: "Scan",
    hint: "Lists the folders directly inside it. Nothing is added or trusted by a scan.",
  },
  workspace: {
    title: "Import a .code-workspace file", label: ".code-workspace file", placeholder: "~/Documents/orbyn.code-workspace", run: "Read file",
    hint: "Lists the folders the file names. Nothing in it is run, and nothing is added or trusted by reading it.",
  },
} as const;

type Added = { row: RepoRow } | { adding: true };

function FolderRow({ folder, added, disabled, onAdd, onTrust }: { folder: ScannedFolder; added: Added | undefined; disabled: boolean; onAdd(): void; onTrust(row: RepoRow): void }) {
  const row = added && "row" in added ? added.row : null;
  return (
    <li className="repos-row repos-scan-row" data-refused={folder.refused ?? undefined}>
      <span className="repos-name">{plain(folder.name)}</span>
      <code className="repos-path" title={plain(folder.path)}>{plain(folder.path)}</code>
      <span className="repos-state">
        {folder.is_git ? <Pill tone="info">Git</Pill> : null}
        {folder.in_project ? <Pill tone="info">Already in a project</Pill> : null}
        {folder.refused ? <span className="repos-scan-why">{REFUSED_TEXT[folder.refused]}</span> : null}
        {row ? <Pill tone={row.trusted ? "accent" : "warn"}>{row.trusted ? "Added, trusted" : "Added, not trusted"}</Pill> : null}
        {row && !row.trusted && row.is_git ? <Button small disabled={disabled} onClick={() => onTrust(row)}>Trust…</Button> : null}
        {!row && canAddScanned(folder) ? <Button small busy={added !== undefined} disabled={disabled} aria-label={`Add ${plain(folder.name)}`} onClick={onAdd}>Add</Button> : null}
      </span>
    </li>
  );
}

/**
 * Scan a folder, or import a `.code-workspace` file, through the one scan route (docs/multi-repo.md). The scan is Person-class and writes
 * nothing; the page reads no disk itself. Each result is added on its own click through add-folder (one write, one busy guard for the whole
 * list), then waits for the existing trust prompt. A refused entry and one already in a project are listed and cannot be added. The newest
 * scan wins: an older answer that arrives late is dropped.
 */
export function ScanDialog({ projectId, mode, onClose, onChanged }: { projectId: string; mode: ScanMode; onClose(): void; onChanged(): void }) {
  const copy = COPY[mode];
  const [path, setPath] = useState("");
  const [scan, setScan] = useState<RepoScan | null>(null);
  const [absent, setAbsent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [picking, setPicking] = useState(false);
  const [added, setAdded] = useState<ReadonlyMap<string, Added>>(new Map());
  const [trusting, setTrusting] = useState<RepoRow | null>(null);
  const latest = useRef<AbortController | null>(null);
  const writing = useRef(false);
  const typed = path.trim();
  useEffect(() => () => latest.current?.abort(), []);

  const run = async () => {
    if (typed === "") return;
    latest.current?.abort();
    const abort = new AbortController();
    latest.current = abort;
    setScanning(true);
    setError(null);
    setAbsent(false);
    try {
      const result = await reposApi().scan(projectId, typed, abort.signal);
      if (abort.signal.aborted) return;
      setAbsent(result === null);
      setScan(result);
      setAdded(new Map());
    } catch (failure) {
      if (abort.signal.aborted) return;
      setScan(null);
      setError(toFailure(failure).message);
    } finally {
      if (latest.current === abort) setScanning(false);
    }
  };

  const browse = async () => {
    if (picking) return;
    setPicking(true);
    setError(null);
    try {
      const chosen = await pickFolder();
      if (chosen !== null) setPath(chosen);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "The folder dialog did not open. Type the path instead.");
    } finally {
      setPicking(false);
    }
  };

  const add = async (folder: ScannedFolder) => {
    if (writing.current) return;
    writing.current = true;
    setError(null);
    setAdded((held) => new Map(held).set(folder.path, { adding: true }));
    try {
      const row = await reposApi().addFolder(projectId, folder.path);
      setAdded((held) => new Map(held).set(folder.path, { row }));
      onChanged();
    } catch (failure) {
      setAdded((held) => { const next = new Map(held); next.delete(folder.path); return next; });
      setError(`Couldn't add ${plain(folder.name)}. ${toFailure(failure).message}`);
    } finally {
      writing.current = false;
    }
  };

  const adding = [...added.values()].some((entry) => "adding" in entry);
  const trusted = (row: RepoRow) => {
    const key = [...added.entries()].find(([, entry]) => "row" in entry && entry.row.id === row.id)?.[0];
    if (key !== undefined) setAdded((held) => new Map(held).set(key, { row: { ...row, trusted: true } }));
    onChanged();
  };
  return (
    <>
      <Dialog title={copy.title} className="repos-dialog repos-scan" busy={adding} onClose={onClose} footer={<Button onClick={onClose} disabled={adding}>Done</Button>}>
        <Field label={copy.label}>
          {(field) => (
            <span className="repos-folder-field">
              <input {...field} data-dialog-focus className="dialog-field" value={path} placeholder={copy.placeholder} spellCheck={false}
                onChange={(event) => setPath(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void run(); } }} />
              {mode === "folder" && canPickFolder() ? <Button type="button" busy={picking} onClick={() => void browse()}>Browse…</Button> : null}
              <Button type="button" tone="primary" disabled={typed === "" || picking} onClick={() => void run()}>{copy.run}</Button>
            </span>
          )}
        </Field>
        <p className="field-hint">{copy.hint}</p>
        {scanning ? <p className="field-hint" role="status">Reading…</p> : null}
        {error ? <p className="field-error" role="alert">{error}</p> : null}
        {absent ? <p className="field-hint" role="status">This version of the service cannot scan a folder. Use Add a folder instead.</p> : null}
        {scan ? (
          <div role="region" aria-label="Scan results">
            <p className="field-hint">
              {scan.code_workspace_file ? <>From <Path path={scan.code_workspace_file} />.</> : <>In <Path path={scan.path} />.</>}
              {" "}{scan.folders.length === 0 ? "No folders found." : scan.folders.length === 1 ? "1 folder found." : `${scan.folders.length} folders found.`}
            </p>
            <ul className="repos-list">
              {scan.folders.map((folder) => (
                <FolderRow key={`${folder.source}:${folder.path}`} folder={folder} added={added.get(folder.path)} disabled={adding || scanning}
                  onAdd={() => void add(folder)} onTrust={setTrusting} />
              ))}
            </ul>
            {scan.truncated ? <p className="field-hint" role="note">There are more folders than are listed here. Scan a smaller folder, or use Add a folder for the one you want.</p> : null}
          </div>
        ) : null}
      </Dialog>
      {trusting ? <TrustDialog projectId={projectId} repo={trusting} onClose={() => setTrusting(null)} onDone={() => trusted(trusting)} /> : null}
    </>
  );
}
