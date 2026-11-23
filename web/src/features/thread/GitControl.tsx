import { DRow } from "../../product/ui/DetailRows";
import { Check, ChevronDown, CloudUpload, GitCommitHorizontal, GitBranch, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "./api";
import { useCommand } from "../../product/shell/shortcuts";
import type { BranchInfo, Thread } from "../../api/types";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useWorkspace } from "../../product/state/workspace";
import { Dialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { GitActionDialog, PublishDialog } from "../panels";
import { plain as plainBranch, RepoChooser, repoLabel, useRepoChoice } from "../repos";
import { PLANS, type Plan } from "../../product/git/plans";

/** The `repo_id` argument, left out altogether for an ordinary thread and for the primary repository. */
const only = (repoId: string | undefined): [] | [string] => (repoId === undefined ? [] : [repoId]);

/** Commit, push and pull request for the open thread, in one menu in its header. */
export function GitActions({ thread }: { thread: Thread }) {
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [publishing, setPublishing] = useState(false);
  // A thread in several repositories acts on the chosen one; any other thread has no choice and no `repo_id`.
  const choice = useRepoChoice(thread.id);
  const branches = useAsync(`git-branches:${thread.id}${choice.repoId === undefined ? "" : `:${choice.repoId}`}`, (signal) => api.branches(thread.id, signal, ...only(choice.repoId)));
  // A repository that can't answer Git questions hides the menu for an ordinary thread; a multi-repository thread keeps the chooser so another repository can be picked.
  if ((branches.status === "error" && !choice.multi) || thread.active_run) return null;
  const hasRemote = branches.data?.some((branch) => branch.remote) ?? false;
  const unavailable = branches.status === "error" ? "Git isn't available in this repository." : undefined;
  const items: MenuItem[] = [
    ...PLANS.map(({ label, plan: picked, icon: Icon }) => ({
      label,
      icon: <Icon size={14} aria-hidden />,
      disabled: unavailable !== undefined,
      title: unavailable,
      onSelect: () => setPlan(picked),
    })),
    {
      label: "Publish repository",
      icon: <CloudUpload size={14} aria-hidden />,
      separatorBefore: true,
      // Publishing belongs to the project's primary repository: the publish route takes no `repo_id`.
      disabled: hasRemote || choice.repoId !== undefined,
      ...(choice.repoId === undefined ? {} : { title: "Publishing applies to the project's primary repository." }),
      onSelect: () => setPublishing(true),
    },
  ];
  return (
    <>
      <RepoChooser choice={choice} />
      <button
        type="button"
        className="git-trigger"
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setAnchor({ x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <GitCommitHorizontal size={14} aria-hidden />
        Git
        <ChevronDown size={12} aria-hidden />
      </button>
      <Presence>{anchor ? <Menu x={anchor.x} y={anchor.y} label="Git actions" {...(choice.selected ? { heading: repoLabel(choice.selected.name) } : {})} items={items} onClose={() => setAnchor(null)} /> : null}</Presence>
      {plan ? <GitActionDialog thread={thread} plan={plan} repo={choice.selected ? { id: choice.repoId, name: repoLabel(choice.selected.name) } : undefined} onClose={() => setPlan(null)} /> : null}
      {publishing ? <PublishDialog thread={thread} onClose={() => setPublishing(false)} /> : null}
    </>
  );
}

function rowsFor(branches: BranchInfo[], query: string) {
  const needle = query.trim().toLowerCase();
  const found = branches.filter((branch) => branch.name.toLowerCase().includes(needle));
  return [...found.filter((branch) => branch.current), ...found.filter((branch) => !branch.current)];
}

/** Lists the checkout's branches. Picking one switches; a new name creates it. */
function BranchDialog({ thread, onClose }: { thread: Thread; onClose(): void }) {
  const choice = useRepoChoice(thread.id);
  const repoId = choice.repoId;
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const branches = useAsync(`branch-list:${thread.id}${repoId === undefined ? "" : `:${repoId}`}`, (signal) => api.branches(thread.id, signal, ...only(repoId)));
  const rows = useMemo(() => rowsFor(branches.data ?? [], query), [branches.data, query]);
  const name = query.trim();
  const exists = (branches.data ?? []).some((branch) => branch.name === name);
  /** Nothing matches the text, so Enter creates it. */
  const creating = Boolean(name) && !exists && rows.length === 0 && branches.status === "ready";
  const act = async (effect: Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await effect;
      void useWorkspace.getState().refreshAll();
      useThreadLogs.getState().touchDiff(thread.id);
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  return (
    <Dialog title="Switch branch" className="branch-dialog" onClose={onClose} busy={busy}>
      {choice.multi ? <div className="branch-repo"><RepoChooser choice={choice} /></div> : null}
      <label className="branch-find">
        <Search size={14} aria-hidden />
        <input
          aria-label="Search branches"
          placeholder="Find a branch"
          value={query}
          autoFocus
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && creating && !busy) {
              event.preventDefault();
              void act(api.createBranch(thread.id, name, ...only(repoId)));
            }
          }}
        />
      </label>
      <ul className="branch-list" aria-label="Branches">
        {rows.map((branch) => (
          <li key={branch.name}>
            <button type="button" disabled={busy || branch.current} onClick={() => void act(api.switchBranch(thread.id, branch.name, ...only(repoId)))}>
              <span className="branch-mark">{branch.current ? <Check size={14} aria-label="Current branch" /> : null}</span>
              <span className="branch-name">{branch.name}</span>
              <span className="branch-note">{branch.current ? "Current" : branch.remote ? "Remote only" : ""}</span>
            </button>
          </li>
        ))}
        {name && !exists ? (
          <li>
            <button type="button" data-active={creating || undefined} disabled={busy} onClick={() => void act(api.createBranch(thread.id, name, ...only(repoId)))}>
              <span className="branch-mark"><Plus size={14} aria-hidden /></span>
              <span className="branch-create">Create branch “{name}”</span>
              {creating ? <span className="branch-note">Enter</span> : null}
            </button>
          </li>
        ) : null}
      </ul>
      <p className="branch-hint">
        {creating ? "No branch matches. Enter creates it from the current branch and switches to it." : "Choosing a branch switches to it."}
      </p>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}

/** The branch switcher's key, kept on the thread's title bar: the details card that holds the branch row is often closed. */
export function BranchShortcut({ thread }: { thread: Thread }) {
  const [open, setOpen] = useState(false);
  useCommand("composer.branch", () => {
    if (thread.active_run) return false;
    setOpen(true);
  });
  return open ? <BranchDialog thread={thread} onClose={() => setOpen(false)} /> : null;
}

/** The branch row of the details card, which opens the switcher. */
export function BranchRow({ thread, branch }: { thread: Thread; branch: string }) {
  const [open, setOpen] = useState(false);
  // In a multi-repository thread the row names the chosen repository's branch.
  const { selected } = useRepoChoice(thread.id);
  const shown = selected ? plainBranch(selected.branch) : branch;
  return (
    <>
      <DRow
        icon={<GitBranch size={16} aria-hidden />}
        label="Branch"
        ariaLabel={`Branch ${shown}`}
        value={<span className="mono">{shown}</span>}
        sub={selected ? repoLabel(selected.name) : thread.cwd}
        title={`${shown} · ${selected ? repoLabel(selected.name) : thread.cwd}`}
        disabled={thread.active_run !== null}
        onClick={() => setOpen(true)}
      />
      {open ? <BranchDialog thread={thread} onClose={() => setOpen(false)} /> : null}
    </>
  );
}
