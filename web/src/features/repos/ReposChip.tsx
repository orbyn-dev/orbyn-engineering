import { Check, Copy, FolderGit2, Plus, X } from "lucide-react";
import { useState } from "react";
import type { Thread } from "../../api/types";
import { toFailure } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { useUi } from "../../product/state/ui";
import { IconButton, Pill } from "../../ui/controls";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { ToolbarPopover } from "../pulls";
import { reposApi, useRepos, useTaskFolder, useThreadRepos } from "./api";
import type { ThreadRepo } from "./contract";
import { canJoinThread, plain } from "./model";
import "./repos.css";

/** The task folder's path as text, with a button that copies it. Nothing here opens it, links it or runs it. */
function TaskFolderLine({ root, branch, count, max }: { root: string; branch: string; count: number; max: number }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(root);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div className="repos-task" aria-label="Task folder">
      <span className="repos-task-label">Task folder</span>
      <code className="repos-path" title={plain(root)}>{plain(root)}</code>
      <IconButton label={copied ? "Copied" : "Copy task folder path"} onClick={() => void copy()}>{copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}</IconButton>
      <span className="field-hint">One branch for every repository: <code className="repos-branch">{plain(branch)}</code></span>
      <span className="field-hint">{count} of {max} repositories</span>
    </div>
  );
}

function List({ thread, repos, reload }: { thread: Thread; repos: ThreadRepo[]; reload(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const { view } = useRepos(thread.project_id);
  // The folder is read again after an add: its repository count is the service's.
  const [added, setAdded] = useState(0);
  const task = useTaskFolder(thread.id, added);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const limit = view?.max_per_thread ?? 8;
  const candidates = (view?.repositories ?? []).filter((row) => canJoinThread(row) && !repos.some((entry) => entry.repo_id === row.id));
  const add = async (repoId: string) => {
    if (busy) return;
    setBusy(true);
    try {
      await reposApi().addThreadRepo(thread.id, repoId);
      setAdded((count) => count + 1);
      reload();
    } catch (failure) {
      pushToast(`Couldn't add it. ${toFailure(failure).message}`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="repos-pop">
      <div className="repos-pop-head">{repos.length === 1 ? "Working in 1 repository" : `Working in ${repos.length} repositories`}</div>
      {task ? <TaskFolderLine root={task.root} branch={task.branch} count={task.repos.length} max={task.max_per_thread} /> : null}
      <ul>
        {repos.map((entry) => (
          <li key={entry.repo_id} className="repos-pop-row">
            <span className="repos-name">{plain(entry.name)}</span>
            <span className="repos-counts"><span aria-label={`${entry.added} added`}>+{entry.added}</span> <span aria-label={`${entry.deleted} deleted`}>−{entry.deleted}</span></span>
            <code className="repos-branch">{plain(entry.branch)}</code>
            <Pill tone={entry.trusted ? "accent" : "warn"}>{entry.trusted ? "Trusted" : "Not trusted"}</Pill>
          </li>
        ))}
      </ul>
      <button type="button" className="repos-pop-add" disabled={busy || candidates.length === 0 || repos.length >= limit} aria-haspopup="menu"
        title={candidates.length === 0 ? "Every trusted repository of the project is already in this thread." : undefined}
        onClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); setMenu({ x: box.left, y: box.top - 4 }); }}>
        <Plus size={14} aria-hidden />Add a repository to this thread…
      </button>
      <p className="field-hint">Each repository has its own branch for this thread</p>
      <Presence>
        {menu ? <Menu x={menu.x} y={menu.y} origin="bottom left" label="Repositories" items={candidates.map((row) => ({ label: plain(row.name), onSelect: () => { void add(row.id); } }))} onClose={() => setMenu(null)} /> : null}
      </Presence>
    </div>
  );
}

/**
 * The header's Repos chip: "Repos 2" for a thread that works in several repositories, and nothing for any other thread. It
 * opens one row per repository as a popover, or a bottom sheet on a phone.
 */
export function ReposChip({ thread }: { thread: Thread }) {
  const { repos, reload } = useThreadRepos(thread.id);
  const [open, setOpen] = useState<{ anchor: HTMLElement; sheet: boolean } | null>(null);
  const sheet = useLayoutWidth() === "phone";
  if (!repos || repos.length < 2) return null;
  return (
    <>
      <button type="button" className="repos-chip" aria-label={`Repos ${repos.length}`} aria-haspopup="dialog" aria-expanded={open !== null}
        onClick={(event) => setOpen(open ? null : { anchor: event.currentTarget, sheet })}>
        <FolderGit2 size={14} aria-hidden />Repos {repos.length}
      </button>
      {open ? (
        <ToolbarPopover anchor={open.anchor} label="Repositories" layout={open.sheet ? "sheet" : "popover"} width={392} closeNow={open.sheet && !sheet} onClose={() => setOpen(null)}>
          {(dismiss) => open.sheet ? (
            <>
              <div className="pulls-sheet-top" data-sheet-handle>
                <span className="pulls-grab" aria-hidden />
                <div className="pulls-sheet-head"><b>Repositories</b><IconButton label="Close" onClick={dismiss}><X size={14} aria-hidden /></IconButton></div>
              </div>
              <div className="pulls-sheet-body"><List thread={thread} repos={repos} reload={reload} /></div>
            </>
          ) : <List thread={thread} repos={repos} reload={reload} />}
        </ToolbarPopover>
      ) : null}
    </>
  );
}
