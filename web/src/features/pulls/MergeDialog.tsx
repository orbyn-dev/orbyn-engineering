import { RefreshCw, TriangleAlert } from "lucide-react";
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
  onStale,
  initialMethod,
  fixedMethod,
  initialAuto,
  initialDeleteBranch,
}: {
  projectId: string;
  detail: PullRequestDetail;
  onClose(): void;
  onMerged(): void;
  /** The pull request moved on after it was read: the page reads it again. */
  onStale(): void;
  initialMethod: string;
  /** On the page the merge menu has chosen the method, so the dialog only says which. */
  fixedMethod?: boolean;
  initialAuto?: boolean;
  /** Where the page already holds the project's choice; otherwise the remembered one. */
  initialDeleteBranch?: boolean;
}) {
  const { number } = detail.pull;
  const [method, setMethod] = useState(initialMethod);
  const [auto, setAuto] = useState(initialAuto ?? false);
  const [deleteBranch, setDeleteBranch] = useState(initialDeleteBranch ?? deleteBranchChoice(projectId));
  const [strip, setStrip] = useState(false);
  const [body, setBody] = useState(detail.body);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  // The head the person was looking at when the dialog opened, whatever the page reads since.
  const [seenHead] = useState(detail.head_sha);
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
        // The commit the person saw, so a push since then is refused rather than merged unseen.
        ...(seenHead ? { expected_head: seenHead } : {}),
      });
      rememberDeleteBranch(projectId, deleteBranch);
      onMerged();
      onClose();
    } catch (failure) {
      const known = toFailure(failure);
      if (known.code === "pull_head_changed") setStale(true);
      else setError(known.message);
      setBusy(false);
    }
  };
  if (stale) {
    return (
      // A key, so the dialog opens afresh and focus lands on Reload.
      <Dialog
        key="stale"
        title="Merge pull request"
        onClose={onClose}
        footer={
          <>
            <Button onClick={onClose}>Cancel</Button>
            <Button
              data-dialog-focus
              tone="primary"
              icon={<RefreshCw size={14} aria-hidden />}
              onClick={() => {
                onStale();
                onClose();
              }}
            >
              Reload
            </Button>
          </>
        }
      >
        <div className="pulls-banner merge-stale" role="alert">
          <TriangleAlert size={14} aria-hidden />
          <span>
            <b>This pull request changed</b>
            <br />
            <span className="merge-stale-line">A new commit was added after you opened this page. Review it again before merging.</span>
          </span>
        </div>
      </Dialog>
    );
  }
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
      {fixedMethod ? null : (
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
      <label className="check">
        <input type="checkbox" checked={auto} onChange={(event) => setAuto(event.target.checked)} />
        <span>Merge when checks pass</span>
      </label>
      <label className="check">
        <input type="checkbox" checked={deleteBranch} onChange={(event) => setDeleteBranch(event.target.checked)} />
        <span>Delete the branch</span>
      </label>
      <label className="check">
        <input type="checkbox" role="switch" aria-describedby="merge-strip-hint" checked={strip} onChange={(event) => setStrip(event.target.checked)} />
        <span>Remove agent credits</span>
      </label>
      <p id="merge-strip-hint" className="field-hint">Strips agent credit lines from the commit message.</p>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}

/** The same dialog from a list row: it reads the pull request first, for its message. */
export function RowMergeDialog({ projectId, number, onClose, onMerged, onStale }: { projectId: string; number: number; onClose(): void; onMerged(): void; onStale(): void }) {
  const detail = useAsync(`pull-merge:${projectId}:${number}`, (signal) => pullsApi.detail(projectId, number, signal));
  if (detail.data) {
    return <MergeDialog projectId={projectId} detail={detail.data} initialMethod="squash" onClose={onClose} onMerged={onMerged} onStale={onStale} />;
  }
  return (
    <Dialog
      title="Merge pull request"
      onClose={onClose}
      footer={
        <>
          <Button data-dialog-focus onClick={onClose}>
            Cancel
          </Button>
          {detail.status === "error" ? (
            <Button tone="primary" onClick={() => void detail.reload()}>
              Try again
            </Button>
          ) : null}
        </>
      }
    >
      {detail.status === "error" ? (
        <p className="field-error" role="alert">
          Couldn{"\u2019"}t load details.
        </p>
      ) : (
        <p className="field-hint" role="status">
          Loading {"\u2026"}
        </p>
      )}
    </Dialog>
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
