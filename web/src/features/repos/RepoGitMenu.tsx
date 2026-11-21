import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import { toFailure } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { Field, IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { ActionDialog } from "./ActionDialog";
import { reposApi } from "./api";
import type { ThreadGitAction, ThreadRepo } from "./contract";
import { ConfirmFacts } from "./Dialogs";
import { plain } from "./model";

type Open = "commit" | "pull_request" | "remove";

/** A pull request address from the service is opened only when it is a plain https one. */
const safeUrl = (url: string) => (/^https:\/\/[^\s]+$/.test(url) ? url : null);

/**
 * The Git menu of ONE repository of a thread, on its group in the Changes list. Every item acts on this repository only: there is
 * no action that spans repositories. A repository that is not trusted runs nothing: its Git items are off and nothing is sent.
 * Push and Pull are one write per click (the ref refuses a second click before the first renders); Commit, Create pull request and
 * Remove ask in a dialog the person confirms, through the shared one-write dialog.
 */
export function RepoGitMenu({ threadId, repo, onChanged }: { threadId: string; repo: ThreadRepo; onChanged(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [open, setOpen] = useState<Open | null>(null);
  const [text, setText] = useState("");
  const running = useRef(false);
  // One choice per opening: a second click on an item while the menu is closing is not a second choice.
  const picked = useRef(false);
  const name = plain(repo.name);
  const run = async (action: ThreadGitAction) => {
    await reposApi().threadGit(threadId, repo.repo_id, action);
    onChanged();
  };
  const quick = async (action: ThreadGitAction & { kind: "push" | "pull" }) => {
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
    { label: "Pull", disabled: !repo.trusted, title: untrusted, onSelect: () => { void quick({ kind: "pull" }); } },
    repo.pull_request
      ? { label: "Open pull request", disabled: pr === null, onSelect: () => { if (pr) window.open(pr, "_blank", "noopener,noreferrer"); } }
      : { label: "Create pull request…", disabled: !repo.trusted, title: untrusted, onSelect: () => { setText(""); setOpen("pull_request"); } },
    { label: "Remove from this thread", destructive: true, separatorBefore: true, onSelect: () => setOpen("remove") },
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
          <p className="field-hint">Opens one pull request for {plain(repo.branch)} in {name}.</p>
        </ActionDialog>
      ) : null}
      {open === "remove" ? (
        <ActionDialog title={`Remove ${name} from this thread?`} confirmLabel="Remove" tone="danger" cancelFocus onClose={close}
          onConfirm={async () => { await reposApi().removeThreadRepo(threadId, repo.repo_id); onChanged(); }}>
          <p>The thread stops working in {name}. The worktree's files stay for you to decide.</p>
          <ConfirmFacts facts={[{ label: "Branch", value: plain(repo.branch), mono: true }]} />
        </ActionDialog>
      ) : null}
    </>
  );
}
