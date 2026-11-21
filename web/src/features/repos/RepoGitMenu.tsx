import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
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

type Open = "commit" | "pull_request" | "merge" | "review";

/** A pull request address from the service is opened only when it is a plain https one. */
const safeUrl = (url: string) => (/^https:\/\/[^\s]+$/.test(url) ? url : null);

/**
 * The Git menu of ONE repository of a thread, on its group in the Changes list. Every item acts on this repository only: there is
 * no action that spans repositories, and no item removes a repository from a thread (not in the beta). Each item sends the thread's
 * existing route with this repository's `repo_id`, left out for the project's primary repository. A repository that is not trusted
 * runs nothing: its Git items are off and nothing is sent; one the service finds untrusted (`409 repo_untrusted`) opens the app's own
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
  const [method, setMethod] = useState<"merge" | "squash" | "rebase">("merge");
  const [event, setEvent] = useState<"comment" | "approve" | "request_changes">("comment");
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
  const pr = repo.pull_request ? safeUrl(repo.pull_request.url) : null;
  const untrusted = repo.trusted ? undefined : "This repository is not trusted. Trust it in the project's settings first.";
  const once = (item: MenuItem): MenuItem => ({ ...item, onSelect: () => { if (picked.current) return; picked.current = true; item.onSelect?.(); } });
  const items: MenuItem[] = [
    { label: "Commit…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setOpen("commit"); } },
    { label: "Push", disabled: !repo.trusted, title: untrusted, onSelect: () => { void quick({ kind: "push" }); } },
    repo.pull_request
      ? { label: "Open pull request", disabled: pr === null, onSelect: () => { if (pr) window.open(pr, "_blank", "noopener,noreferrer"); } }
      : { label: "Create pull request…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setOpen("pull_request"); } },
    ...(repo.pull_request ? [
      { label: "Merge pull request…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setMethod("merge"); setOpen("merge"); } },
      { label: "Review pull request…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setEvent("comment"); setOpen("review"); } },
    ] : []),
  ].map(once);
  const close = () => setOpen(null);
  const number = repo.pull_request?.number ?? 0;
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
      {open === "merge" ? (
        <ActionDialog title={`Merge the pull request for ${name}`} confirmLabel="Merge" onClose={close}
          onConfirm={() => run({ kind: "merge", number, method })}>
          <Field label="Method">{(field) => (
            <select {...field} data-dialog-focus className="dialog-field" value={method} onChange={(event) => setMethod(event.target.value as typeof method)}>
              <option value="merge">Merge commit</option>
              <option value="squash">Squash and merge</option>
              <option value="rebase">Rebase and merge</option>
            </select>
          )}</Field>
          <p className="field-hint">Merges pull request #{number} of {name} only.</p>
        </ActionDialog>
      ) : null}
      {open === "review" ? (
        <ActionDialog title={`Review the pull request for ${name}`} confirmLabel="Send review" disabled={event !== "approve" && text.trim() === ""} onClose={close}
          onConfirm={() => run({ kind: "review", number, event, body: text.trim() })}>
          <Field label="Decision">{(field) => (
            <select {...field} data-dialog-focus className="dialog-field" value={event} onChange={(change) => setEvent(change.target.value as typeof event)}>
              <option value="comment">Comment</option>
              <option value="approve">Approve</option>
              <option value="request_changes">Request changes</option>
            </select>
          )}</Field>
          <Field label="Comment">{(field) => <input {...field} className="dialog-field" value={text} onChange={(change) => setText(change.target.value)} />}</Field>
          <p className="field-hint">Reviews pull request #{number} of {name} only.</p>
        </ActionDialog>
      ) : null}
      {trust && projectId ? (
        <TrustDialog projectId={projectId} repo={trust} onClose={() => setTrust(null)} onDone={() => { pushToast(`Trusted ${plain(trust.name)}. Run the action again.`); onChanged(); }} />
      ) : null}
    </>
  );
}
