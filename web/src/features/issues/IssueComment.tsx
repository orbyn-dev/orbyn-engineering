import { useId, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button } from "../../ui/controls";
import { issuesApi, markNotOffered, useCommentRefused } from "./api";
import type { IssueDetail } from "./contract";
import { COMMENT_MAX, byteLength, commentProblem, routeAbsent } from "./edit";
import type { IssueWriter } from "./IssueEdit";
import { reasonOf } from "./model";
import { PlannedNote } from "./Planned";

/**
 * The comment box under an issue's comments, the way the issue page's other editors are drawn: a minimal inline form (no
 * frame draws it yet). It follows their rules:
 *
 * - The text is the person's, and it goes out as the `body` field of `POST /api/issues/item/comment`, never as markup.
 * - One click is one write, through the page's own guard: while a write is in flight a second click, or another control's
 *   write, sends nothing.
 * - A failed write loses nothing: the draft stays exactly as it was typed and the form says nothing changed.
 * - A blank comment, or one over 64 KiB, is refused on this side before anything is sent.
 * - A 404 or 405 that is not `not_found` means this service has no comment route: the box goes back to Planned and stays
 *   there for the life of this client.
 */
export function CommentBox({ issue, write, writing }: { issue: IssueDetail; write: IssueWriter; writing: boolean }) {
  const refused = useCommentRefused();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const field = useId();
  const bytes = byteLength(draft);
  const problem = commentProblem(draft);
  // An untouched box is not a mistake: the empty comment is said only once something was typed and taken back.
  const shown = draft === "" ? null : problem;
  const save = async () => {
    if (writing || problem) return;
    setError(null);
    const text = draft;
    try {
      // The route answers the stored comment; the page shows it where the tracker's comments are, oldest first.
      const done = await write(async () => {
        const posted = await issuesApi().comment(issue, text);
        return { ...issue, comments: [...issue.comments, posted], comment_count: issue.comment_count + 1 };
      });
      if (done) setDraft("");
    } catch (failure) {
      if (failure instanceof ApiFailure && routeAbsent(failure.status, failure.code)) {
        markNotOffered("comment");
        return;
      }
      // The draft stays exactly as it was typed.
      setError(`${reasonOf(failure)} Nothing changed.`);
    }
  };
  if (refused) {
    return (
      <form className="iss-edit iss-comment" aria-label="Comment">
        <label htmlFor={field} className="iss-edit-label">Comment</label>
        <textarea id={field} rows={4} disabled />
        <div className="iss-edit-foot">
          <span className="iss-edit-count iss-quiet">Commenting is not available in this version.</span>
          <Button small type="submit" disabled>Comment</Button>
          <PlannedNote what="Commenting" />
        </div>
      </form>
    );
  }
  return (
    <form className="iss-edit iss-comment" aria-label="Comment" onSubmit={(event) => { event.preventDefault(); void save(); }}>
      <label htmlFor={field} className="iss-edit-label">Comment</label>
      <textarea
        id={field}
        rows={4}
        value={draft}
        disabled={writing}
        aria-invalid={shown !== null || undefined}
        onChange={(event) => setDraft(event.target.value)}
      />
      <div className="iss-edit-foot">
        <span className={problem ? "iss-edit-count iss-edit-over" : "iss-edit-count iss-quiet"}>{Math.ceil(bytes / 1024)} of {COMMENT_MAX / 1024} KiB</span>
        <Button small tone="primary" type="submit" busy={writing} disabled={problem !== null}>Comment</Button>
      </div>
      {shown ? <p className="iss-edit-error" role="alert">{shown}</p> : null}
      {error ? <p className="iss-edit-error" role="alert">{error}</p> : null}
    </form>
  );
}
