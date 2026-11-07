import { useState } from "react";
import { pullsApi } from "../../api/pulls";
import type { PullRequestDetail } from "../../api/types";
import { toFailure, useAsync } from "../../lib/useAsync";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";

const METHOD_WORDS: Record<string, string> = { squash: "Squash and merge", merge: "Create a merge commit", rebase: "Rebase and merge" };

export function MergeDialog({
  projectId,
  detail,
  onClose,
  onMerged,
  initialMethod,
  chosen,
}: {
  projectId: string;
  detail: PullRequestDetail;
  onClose(): void;
  onMerged(): void;
  initialMethod: string;
  /** On the page the merge menu has already chosen these, so the dialog asks only for the message. */
  chosen?: { auto: boolean; deleteBranch: boolean };
}) {
  const { number } = detail.pull;
  const [method, setMethod] = useState(initialMethod);
  const [auto, setAuto] = useState(chosen?.auto ?? false);
  const [deleteBranch, setDeleteBranch] = useState(chosen?.deleteBranch ?? true);
  const [strip, setStrip] = useState(false);
  const [body, setBody] = useState(detail.body);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const merge = async () => {
    setBusy(true);
    setError(null);
    try {
      await pullsApi.merge(projectId, number, {
        method,
        auto,
        delete_branch: deleteBranch,
        strip_credits: strip,
        ...(method === "rebase" ? {} : { body }),
      });
      onMerged();
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="Merge pull request"
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button data-dialog-focus onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} onClick={() => void merge()}>
            {auto ? "Enable auto-merge" : "Merge"}
          </Button>
        </>
      }
    >
      <p className="field-hint">
        {METHOD_WORDS[method] ?? "Squash and merge"} #{number} into {detail.pull.base}.
      </p>
      {chosen ? null : (
        <Field label="Merge method">
          {(props) => (
            <select {...props} value={method} onChange={(event) => setMethod(event.target.value)}>
              <option value="squash">Squash and merge</option>
              <option value="merge">Create a merge commit</option>
              <option value="rebase">Rebase and merge</option>
            </select>
          )}
        </Field>
      )}
      {method === "rebase" ? null : (
        <Field label="Commit message">
          {(props) => <textarea {...props} rows={4} value={body} onChange={(event) => setBody(event.target.value)} />}
        </Field>
      )}
      {chosen ? null : (
        <>
          <label className="check">
            <input type="checkbox" checked={auto} onChange={(event) => setAuto(event.target.checked)} />
            <span>Merge when checks pass</span>
          </label>
          <label className="check">
            <input type="checkbox" checked={deleteBranch} onChange={(event) => setDeleteBranch(event.target.checked)} />
            <span>Delete the branch</span>
          </label>
        </>
      )}
      <label className="check">
        <input type="checkbox" role="switch" checked={strip} onChange={(event) => setStrip(event.target.checked)} />
        <span>Remove agent credits</span>
      </label>
      <p className="field-hint">Strips agent credit lines from the commit message.</p>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}

/** The same dialog from a list row: it reads the pull request first, for its message. */
export function RowMergeDialog({ projectId, number, onClose, onMerged }: { projectId: string; number: number; onClose(): void; onMerged(): void }) {
  const detail = useAsync(`pull-merge:${projectId}:${number}`, (signal) => pullsApi.detail(projectId, number, signal));
  if (!detail.data) return null;
  return (
    <MergeDialog
      projectId={projectId}
      detail={detail.data}
      initialMethod="squash"
      chosen={{ auto: false, deleteBranch: deleteBranchChoice(projectId) }}
      onClose={onClose}
      onMerged={onMerged}
    />
  );
}

const DELETE_KEY = (projectId: string) => `orbyn.pulls.deleteBranch.${projectId}`;
/** "Delete branch after merge" is off until chosen, and then remembered for the project. */
export function deleteBranchChoice(projectId: string): boolean {
  try {
    return localStorage.getItem(DELETE_KEY(projectId)) === "1";
  } catch {
    return false;
  }
}
export function rememberDeleteBranch(projectId: string, on: boolean) {
  try {
    localStorage.setItem(DELETE_KEY(projectId), on ? "1" : "0");
  } catch {
    /* A private window keeps nothing: the choice then lasts for this page. */
  }
}
