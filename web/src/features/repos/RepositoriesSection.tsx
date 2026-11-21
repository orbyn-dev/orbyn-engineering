import { Ellipsis, GitBranch, Plus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, IconButton, Pill } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { useRepos } from "./api";
import type { RepoRow, ReposView } from "./contract";
import { AddFolderDialog, CancelCloneDialog, CloneDialog, CreateDialog, RemoveDialog, SetupGitDialog, TrustDialog } from "./Dialogs";
import { PublishDialog } from "./Publish";
import { ScanDialog, type ScanMode } from "./Scan";
import { cloneText, plain, repoState, STATE_LABEL } from "./model";
import "./repos.css";

type Open =
  | { kind: "add-folder" | "clone" | "create" }
  | { kind: "scan"; mode: ScanMode }
  | { kind: "publish" | "remove" | "trust" | "git" | "cancel-clone"; repo: RepoRow };

const CLONE_POLL_MS = 2000;

function Row({ row, disabled, onAction, onMenu }: { row: RepoRow; disabled: boolean; onAction(kind: "trust" | "git" | "cancel-clone", row: RepoRow): void; onMenu(anchor: HTMLElement, row: RepoRow): void }) {
  const state = repoState(row);
  const note = cloneText(row);
  return (
    <li className="repos-row" data-state={state}>
      <span className="repos-name">{plain(row.name)}</span>
      <code className="repos-path" title={plain(row.path)}>{plain(row.path)}</code>
      <span className="repos-branch">
        {row.branch ? <><GitBranch size={13} aria-hidden />{plain(row.branch)}</> : "No branch"}
      </span>
      <span className="repos-state">
        {note ? <Pill tone={row.clone?.state === "failed" ? "danger" : "info"}>{note}</Pill> : <Pill tone={state === "trusted" ? "accent" : "warn"}>{STATE_LABEL[state]}</Pill>}
        {row.clone?.state === "cloning" ? <Button small disabled={disabled} onClick={() => onAction("cancel-clone", row)}>Cancel</Button> : null}
        {!note && state === "untrusted" ? <Button small disabled={disabled} onClick={() => onAction("trust", row)}>Trust…</Button> : null}
        {!note && state === "not_git" ? <Button small disabled={disabled} onClick={() => onAction("git", row)}>Set up Git</Button> : null}
        <IconButton label={`Actions for ${plain(row.name)}`} aria-haspopup="menu" disabled={disabled} onClick={(event) => onMenu(event.currentTarget, row)}>
          <Ellipsis size={16} aria-hidden />
        </IconButton>
      </span>
    </li>
  );
}

/**
 * The Repositories group of project settings. It draws only while the service reports the project's repositories; on the
 * default client it draws nothing and makes no request. Every write goes through a dialog the person confirms, one click one
 * write, and an untrusted repository is only ever trusted by its own dialog.
 */
export function RepositoriesSection({ projectId, projectName, disabled = false }: { projectId: string; projectName: string; disabled?: boolean }) {
  const { view, reload } = useRepos(projectId);
  const kept = useRef<ReposView | null>(null);
  if (view) kept.current = view;
  const shown = view ?? kept.current;
  const [open, setOpen] = useState<Open | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const cloning = shown?.repositories.some((row) => row.clone?.state === "cloning") ?? false;
  useEffect(() => {
    if (!cloning) return;
    const timer = setInterval(reload, CLONE_POLL_MS);
    return () => clearInterval(timer);
  }, [cloning, reload]);
  if (!shown) return null;
  const done = () => { setOpen(null); reload(); };
  const place = (anchor: HTMLElement) => { const box = anchor.getBoundingClientRect(); return { x: box.left, y: box.bottom + 4 }; };
  const rowItems = (row: RepoRow): MenuItem[] => {
    const state = repoState(row);
    return [
      ...(state === "trusted" && !row.has_remote && row.clone === null ? [{ label: "Publish to GitHub…", onSelect: () => setOpen({ kind: "publish", repo: row }) }] : []),
      ...(state === "untrusted" ? [{ label: "Trust this folder…", onSelect: () => setOpen({ kind: "trust", repo: row }) }] : []),
      { label: "Remove from project…", destructive: true, separatorBefore: true, onSelect: () => setOpen({ kind: "remove", repo: row }) },
    ];
  };
  const common = { projectId, onClose: () => setOpen(null), onDone: done };
  return (
    <section className="setting-group repos" aria-label="Repositories">
      <div className="set-bh"><b>Repositories <Pill tone="info">Beta</Pill></b></div>
      <p className="setting-line-desc">The folders of this project. A thread can work in one or several.</p>
      <ul className="repos-list">
        {shown.repositories.map((row) => (
          <Row key={row.id} row={row} disabled={disabled} onAction={(kind, target) => setOpen({ kind, repo: target })}
            onMenu={(anchor, target) => setMenu({ ...place(anchor), items: rowItems(target) })} />
        ))}
      </ul>
      {shown.repositories.length === 0 ? <p className="field-hint">No repositories yet.</p> : null}
      <div className="form-actions">
        <Button small disabled={disabled} icon={<Plus size={13} aria-hidden />} aria-haspopup="menu"
          onClick={(event) => setMenu({ ...place(event.currentTarget), items: [
            { label: "Add a folder…", title: "A folder on this computer", onSelect: () => setOpen({ kind: "add-folder" }) },
            { label: "Scan a folder…", title: "List the folders inside one", onSelect: () => setOpen({ kind: "scan", mode: "folder" }) },
            { label: "Import a .code-workspace file…", title: "The folders a workspace file names", onSelect: () => setOpen({ kind: "scan", mode: "workspace" }) },
            { label: "Create a new folder…", title: "A new folder with Git", onSelect: () => setOpen({ kind: "create" }) },
            { label: "Clone a repository…", title: "Git URL", onSelect: () => setOpen({ kind: "clone" }) },
          ] })}>Add repository</Button>
      </div>
      <Presence>
        {menu ? <Menu x={menu.x} y={menu.y} label="Repository" items={menu.items} onClose={() => setMenu(null)} /> : null}
      </Presence>
      {open?.kind === "add-folder" ? <AddFolderDialog {...common} /> : null}
      {open?.kind === "scan" ? <ScanDialog projectId={projectId} mode={open.mode} onClose={() => setOpen(null)} onChanged={reload} /> : null}
      {open?.kind === "clone" ? <CloneDialog {...common} projectName={projectName} directory={shown.clone_directory} /> : null}
      {open?.kind === "create" ? <CreateDialog {...common} projectName={projectName} directory={shown.clone_directory} /> : null}
      {open?.kind === "cancel-clone" ? <CancelCloneDialog {...common} repo={open.repo} /> : null}
      {open?.kind === "publish" ? <PublishDialog {...common} onDone={reload} repo={open.repo} /> : null}
      {open?.kind === "remove" ? <RemoveDialog {...common} projectName={projectName} repo={open.repo} /> : null}
      {open?.kind === "trust" ? <TrustDialog {...common} repo={open.repo} /> : null}
      {open?.kind === "git" ? <SetupGitDialog {...common} repo={open.repo} /> : null}
    </section>
  );
}
