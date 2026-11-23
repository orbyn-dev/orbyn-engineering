import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { pullsApi } from "../../api/pulls";
import { toFailure } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Field, IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { ActionDialog } from "./ActionDialog";
import { reposApi } from "./api";
import type { RepoRow, ThreadGitAction, ThreadRepo } from "./contract";
import { TrustDialog } from "./Dialogs";
import { plain } from "./model";
import { RepoMergeDialog, RepoReviewDialog } from "./RepoPullDialogs";

type Open = "commit" | "pull_request" | "review" | "merge";
const STATE_WORDS: Record<string, string> = { open: "Open", merged: "Merged", closed: "Closed" };

/**
 * The Git menu of ONE repository of a thread, on its group in the Changes list. Every item acts on this repository only: there is
 * no action that spans repositories, and no item removes a repository from a thread (that is the Repos popover). Each item sends the thread's
 * existing `git/action` route with this repository's `repo_id`, left out for the project's primary repository. The repository's own pull
 * request (`ThreadRepo.pull_request`: its open one with the highest number, else its latest) is named in the menu; while it is open the
 * menu offers Review and Merge on it, through the thread pull request routes with the same `repo_id`, and no second Create. A repository that is not trusted
 * runs nothing: its Git and pull request items are off and nothing is sent; one the service finds untrusted (`409 repo_untrusted`) opens the app's own
 * trust prompt. Push is one write per click (the ref refuses a second click before the first renders); the rest ask in a dialog the
 * person confirms, through the shared one-write dialog.
 */
export function RepoGitMenu({ threadId, repo, onChanged }: { threadId: string; repo: ThreadRepo; onChanged(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [open, setOpen] = useState<Open | null>(null);
  const [trust, setTrust] = useState<RepoRow | null>(null);
  const [text, setText] = useState("");
  const running = useRef(false);
  // One choice per opening: a second click on an item while the menu is closing is not a second choice.
  const picked = useRef(false);
  const name = plain(repo.name);
  // The project's own repository has the project's id; the service takes no `repo_id` for it.
  const primary = projectId !== undefined && repo.repo_id === projectId;
  /** `true` when the failure was one the person can act on here, and so is already answered. */
  const answered = async (failure: unknown): Promise<boolean> => {
    if (!(failure instanceof ApiFailure)) return false;
    if (failure.status === 404 && failure.code === "not_found") {
      pushToast(`${name} isn't one of this thread's repositories.`);
      return true;
    }
    if (failure.status === 409 && failure.code === "repo_untrusted") {
      const row = projectId ? (await reposApi().list(projectId).catch(() => null))?.repositories.find((one) => one.id === repo.repo_id) : undefined;
      if (row) setTrust(row);
      else pushToast(`${name} is not trusted. Trust it in the project's settings first.`);
      return true;
    }
    return false;
  };
  const run = async (action: ThreadGitAction) => {
    try {
      await reposApi().threadGit(threadId, primary ? null : repo.repo_id, action);
    } catch (failure) {
      // The dialog that asked closes: the trust prompt or the toast answers instead.
      if (await answered(failure)) { setOpen(null); return; }
      throw failure;
    }
    onChanged();
  };
  const quick = async (action: ThreadGitAction & { kind: "push" }) => {
    if (running.current || !repo.trusted) return;
    running.current = true;
    try {
      await run(action);
    } catch (failure) {
      pushToast(`Couldn't ${action.kind} ${name}. ${toFailure(failure).message}`);
    } finally {
      running.current = false;
    }
  };
  const pull = repo.pull_request ?? null;
  const target = pull ? { threadId, number: Number(pull.number), repoId: primary ? undefined : repo.repo_id, name } : null;
  const isOpen = pull?.state === "open";
  /** A merge or review changes the pull request: the forge is asked again before the list reads the repository's state. */
  const pullDone = async () => {
    await pullsApi.refreshLinked(threadId, target?.repoId).catch(() => []);
    onChanged();
  };
  const untrusted = repo.trusted ? undefined : "This repository is not trusted. Trust it in the project's settings first.";
  const once = (item: MenuItem): MenuItem => ({ ...item, onSelect: () => { if (picked.current) return; picked.current = true; item.onSelect?.(); } });
  const items: MenuItem[] = [
    { label: "Commit…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setOpen("commit"); } },
    { label: "Push", disabled: !repo.trusted, title: untrusted, onSelect: () => { void quick({ kind: "push" }); } },
    ...(pull ? [{ label: `Pull request #${Number(pull.number)}`, disabled: true, title: pull.title ? plain(pull.title) : undefined, shortcut: STATE_WORDS[pull.state] ?? plain(pull.state) }] : []),
    ...(isOpen ? [
      { label: "Review…", disabled: !repo.trusted, title: untrusted, onSelect: () => setOpen("review") },
      { label: "Merge…", disabled: !repo.trusted, title: untrusted, onSelect: () => setOpen("merge") },
    ] : [{ label: "Create pull request…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setOpen("pull_request"); } }]),
  ].map(once);
  const close = () => setOpen(null);
  return (
    <>
      <IconButton label={`Git actions for ${name}`} aria-haspopup="menu" onClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); picked.current = false; setMenu({ x: box.left, y: box.bottom + 4 }); }}>
        <Ellipsis size={14} aria-hidden />
      </IconButton>
      <Presence>
        {menu ? <Menu x={menu.x} y={menu.y} label={`Git menu for ${name}`} heading={`${name} · ${plain(repo.branch)}`} items={items} onClose={() => setMenu(null)} /> : null}
      </Presence>
      {open === "commit" ? (
        <ActionDialog title={`Commit ${name}`} confirmLabel="Commit" disabled={text.trim() === ""} onClose={close}
          onConfirm={() => run({ kind: "commit", message: text.trim() })}>
          <Field label="Message">{(field) => <input {...field} data-dialog-focus className="dialog-field" value={text} onChange={(event) => setText(event.target.value)} />}</Field>
          <p className="field-hint">Commits the changes in {name} only, on {plain(repo.branch)}.</p>
        </ActionDialog>
      ) : null}
      {open === "pull_request" ? (
        <ActionDialog title={`Create a pull request for ${name}`} confirmLabel="Create pull request" disabled={text.trim() === ""} onClose={close}
          onConfirm={() => run({ kind: "pull_request", title: text.trim() })}>
          <Field label="Title">{(field) => <input {...field} data-dialog-focus className="dialog-field" value={text} onChange={(event) => setText(event.target.value)} />}</Field>
          <p className="field-hint">Pushes {plain(repo.branch)} and opens one pull request in {name}.</p>
        </ActionDialog>
      ) : null}
      {open === "review" && target ? <RepoReviewDialog pull={target} onClose={close} onDone={() => pullDone()} /> : null}
      {open === "merge" && target ? <RepoMergeDialog pull={target} onClose={close} onDone={() => pullDone()} /> : null}
      {trust && projectId ? (
        <TrustDialog projectId={projectId} repo={trust} onClose={() => setTrust(null)} onDone={() => { pushToast(`Trusted ${plain(trust.name)}. Run the action again.`); onChanged(); }} />
      ) : null}
    </>
  );
}
