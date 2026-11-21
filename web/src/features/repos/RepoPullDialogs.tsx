import { useState } from "react";
import { pullsApi } from "../../api/pulls";
import { Field } from "../../ui/controls";
import { ActionDialog } from "./ActionDialog";

/** What a dialog needs to act on ONE repository's pull request: the thread, the number and the `repo_id` (absent for the primary). */
export interface RepoPull {
  threadId: string;
  number: number;
  repoId: string | undefined;
  /** The repository's name, plain text, for the title. */
  name: string;
}

const REVIEWS = [
  { value: "approve", label: "Approve" },
  { value: "comment", label: "Comment" },
  { value: "request_changes", label: "Request changes" },
];

/** A review of the repository's pull request. Approve needs no note; a comment or requested changes need one. One click, one write. */
export function RepoReviewDialog({ pull, onClose, onDone }: { pull: RepoPull; onClose(): void; onDone(): void }) {
  const [event, setEvent] = useState("approve");
  const [note, setNote] = useState("");
  return (
    <ActionDialog title={`Review pull request #${pull.number} in ${pull.name}`} confirmLabel="Submit review" disabled={event !== "approve" && note.trim() === ""} onClose={onClose}
      onConfirm={async () => { await pullsApi.threadReview(pull.threadId, pull.number, event, note.trim(), pull.repoId); onDone(); }}>
      <Field label="Review">
        {(field) => (
          <select {...field} data-dialog-focus value={event} onChange={(change) => setEvent(change.target.value)}>
            {REVIEWS.map((one) => <option key={one.value} value={one.value}>{one.label}</option>)}
          </select>
        )}
      </Field>
      <Field label="Note">{(field) => <textarea {...field} rows={3} value={note} onChange={(change) => setNote(change.target.value)} />}</Field>
      <p className="field-hint">Reviews pull request #{pull.number} in {pull.name} only.</p>
    </ActionDialog>
  );
}

/**
 * Merges the repository's pull request now. The credit lines follow the account setting (no `remove_agent_credits` is sent):
 * the project's own merge dialog is where that choice is made per merge.
 */
export function RepoMergeDialog({ pull, onClose, onDone }: { pull: RepoPull; onClose(): void; onDone(): void }) {
  const [method, setMethod] = useState("squash");
  const [deleteBranch, setDeleteBranch] = useState(false);
  return (
    <ActionDialog title={`Merge pull request #${pull.number} in ${pull.name}`} confirmLabel="Merge" onClose={onClose}
      onConfirm={async () => { await pullsApi.threadMerge(pull.threadId, pull.number, { method, auto: false, delete_branch: deleteBranch, strip_credits: false }, pull.repoId); onDone(); }}>
      <Field label="Merge method">
        {(field) => (
          <select {...field} data-dialog-focus value={method} onChange={(change) => setMethod(change.target.value)}>
            <option value="squash">Squash and merge</option>
            <option value="merge">Create a merge commit</option>
            <option value="rebase">Rebase and merge</option>
          </select>
        )}
      </Field>
      <label className="check">
        <input type="checkbox" checked={deleteBranch} onChange={(change) => setDeleteBranch(change.target.checked)} />
        <span>Delete the branch</span>
      </label>
      <p className="field-hint">Merges now, in {pull.name} only. Credit lines follow your account setting.</p>
    </ActionDialog>
  );
}
