import { Check, GitCommitHorizontal, GitPullRequest, Upload, X } from "lucide-react";
import { useState } from "react";
import { api } from "../../../api/client";
import type { GitActionResult, GitPhase, Thread } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button, Field } from "../../../ui/controls";
import { Dialog } from "../../../ui/Dialog";

export interface Plan {
  title: string;
  commit: boolean;
  push: boolean;
  pull: boolean;
}

export const PLANS: { label: string; plan: Plan; icon: typeof Upload }[] = [
  { label: "Commit", plan: { title: "Commit", commit: true, push: false, pull: false }, icon: GitCommitHorizontal },
  { label: "Commit and push", plan: { title: "Commit and push", commit: true, push: true, pull: false }, icon: Upload },
  {
    label: "Commit, push and create pull request",
    plan: { title: "Commit, push and create pull request", commit: true, push: true, pull: true },
    icon: GitPullRequest,
  },
  { label: "Push", plan: { title: "Push", commit: false, push: true, pull: false }, icon: Upload },
  {
    label: "Create pull request",
    plan: { title: "Create pull request", commit: false, push: true, pull: true },
    icon: GitPullRequest,
  },
];

/** The menu's own "Create pull request" entry, for other places that offer it. */
export const createPullRequestPlan = (): Plan => PLANS.find((entry) => entry.label === "Create pull request")!.plan;

/** False when the checkout can't answer Git questions, where the header menu hides too. */
export function useGitAvailable(threadId: string): boolean {
  const branches = useAsync(`git-branches:${threadId}`, (signal) => api.branches(threadId, signal));
  return branches.status !== "error";
}

const PHASE_NAMES: Record<string, string> = { commit: "Commit", push: "Push", pull_request: "Pull request" };

function Phase({ phase }: { phase: GitPhase }) {
  return (
    <li>
      {phase.ok ? (
        <Check size={14} className="tone-success" role="img" aria-label="Done" />
      ) : (
        <X size={14} className="tone-danger" role="img" aria-label="Failed" />
      )}
      <span>{PHASE_NAMES[phase.name] ?? phase.name}</span>
      {phase.url ? (
        <a href={phase.url} target="_blank" rel="noreferrer">
          {phase.url}
        </a>
      ) : phase.message ? (
        <small>{phase.message}</small>
      ) : null}
    </li>
  );
}

export function GitActionDialog({ thread, plan, onClose }: { thread: Thread; plan: Plan; onClose(): void }) {
  const [message, setMessage] = useState("");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [base, setBase] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GitActionResult | null>(null);
  const branches = useAsync(`git-bases:${thread.id}`, (signal) => api.branches(thread.id, signal));
  const bases = (branches.data ?? []).filter((branch) => !branch.remote && !branch.current);
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const done = await api.gitAction(thread.id, {
        ...(plan.commit ? { commit_message: message } : {}),
        push: plan.push,
        ...(plan.pull ? { pull_request: { title, body, ...(base ? { base } : {}) } } : {}),
      });
      setResult(done);
      useThreadLogs.getState().touchDiff(thread.id);
      void useWorkspace.getState().refreshThread(thread.id);
      // A commit or pull request moves the branch the projects list shows.
      void useWorkspace.getState().refreshManaged();
    } catch (failure) {
      setError(toFailure(failure).message);
    } finally {
      setBusy(false);
    }
  };
  if (result) {
    return (
      <Dialog title={plan.title} onClose={onClose} footer={<Button onClick={onClose}>Close</Button>}>
        {result.notice ? <p role="status" className="muted">{result.notice}</p> : null}
        <ul className="git-phases" aria-label="Result">
          {result.phases.map((phase) => (
            <Phase key={phase.name} phase={phase} />
          ))}
        </ul>
      </Dialog>
    );
  }
  return (
    <Dialog
      title={plan.title}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} onClick={() => void run()}>
            {plan.title}
          </Button>
        </>
      }
    >
      {plan.commit || plan.pull ? <p>Review and confirm your changes. Leave text blank to auto-generate it when your engine supports writing; this uses engine usage. If generation fails, a blank commit message or PR title uses the thread title. Otherwise a blank PR title uses the thread title.</p> : null}
      {plan.commit ? (
        <Field label="Commit message">
          {(props) => <textarea {...props} rows={3} value={message} placeholder="Leave empty to auto-generate" autoFocus onChange={(event) => setMessage(event.target.value)} />}
        </Field>
      ) : null}
      {plan.pull ? (
        <>
          <Field label="Pull request title">
            {(props) => <input {...props} value={title} placeholder="Leave empty to auto-generate" onChange={(event) => setTitle(event.target.value)} />}
          </Field>
          <Field label="Description">
            {(props) => <textarea {...props} rows={4} value={body} placeholder="Leave empty to auto-generate" onChange={(event) => setBody(event.target.value)} />}
          </Field>
          <Field label="Base branch">
            {(props) => (
              <select {...props} value={base} onChange={(event) => setBase(event.target.value)}>
                <option value="">Repository default</option>
                {bases.map((branch) => (
                  <option key={branch.name} value={branch.name}>
                    {branch.name}
                  </option>
                ))}
              </select>
            )}
          </Field>
        </>
      ) : null}
      {!plan.commit && !plan.pull ? <p>Pushes the current branch to origin.</p> : null}
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}

export function PublishDialog({ thread, onClose }: { thread: Thread; onClose(): void }) {
  const project = useWorkspace((state) => state.projects.data?.find((entry) => entry.id === thread.project_id));
  const [name, setName] = useState(project?.name ?? "");
  const [isPrivate, setPrivate] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pushToast = useUi((state) => state.pushToast);
  const publish = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.publishProject(thread.project_id, { name, private: isPrivate });
      pushToast("Published");
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="Publish repository"
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} disabled={!name.trim()} onClick={() => void publish()}>
            Publish
          </Button>
        </>
      }
    >
      <Field label="Repository name">
        {(props) => <input {...props} value={name} onChange={(event) => setName(event.target.value)} />}
      </Field>
      <label className="check">
        <input type="checkbox" checked={isPrivate} onChange={(event) => setPrivate(event.target.checked)} />
        <span>Private</span>
      </label>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}
