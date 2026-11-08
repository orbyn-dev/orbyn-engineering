import { Lock, RefreshCw, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { pullsApi } from "../../api/pulls";
import { toFailure } from "../../lib/useAsync";
import { Button } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";

/**
 * The review card under the merge row. The note is required, and Escape asks before throwing one away.
 * An answer that is not "sent" keeps the text and says so.
 */
export function RequestChangesCard({
  projectId,
  number,
  own,
  ownKnown,
  note,
  onNote,
  forge,
  onCancel,
  onDone,
  onUnknown,
}: {
  projectId: string;
  number: number;
  /** The person opened this pull request, which a forge does not let them review. */
  own: boolean;
  /** Who is signed in is read: until then the card cannot tell if this pull request is the person's own. */
  ownKnown: boolean;
  /** The note lives with the page, so closing the card and opening it again keeps it. */
  note: string;
  onNote(next: string): void;
  forge: string;
  onCancel(): void;
  onDone(note: string): void;
  /** The forge did not answer: the review may have gone through, so the page reads its reviews again. */
  onUnknown(): void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const send = async () => {
    setBusy(true);
    setError(null);
    try {
      await pullsApi.review(projectId, number, "request_changes", note.trim());
      onDone(note.trim());
    } catch (failure) {
      const known = toFailure(failure);
      // No answer, a timeout or a gateway error: the forge may have taken the review.
      const unknown = known.status === 0 || known.status === 408 || known.status >= 502;
      setError(
        known.code === "unsupported"
          ? `${forge} doesn’t provide that here.`
          : unknown
            ? "Couldn’t confirm the review. It may have been sent: check the conversation before trying again."
            : "Couldn’t submit the review. Nothing was sent. Try again.",
      );
      if (unknown) onUnknown();
      setBusy(false);
    }
  };
  const leave = () => (note.trim() ? setDiscarding(true) : onCancel());
  return (
    <section
      className="pull-card pull-review"
      aria-label="Request changes"
      onKeyDown={(event) => {
        // From anywhere in the card: Escape closes it, and asks first when there is a note.
        if (event.key === "Escape" && !discarding) {
          event.stopPropagation();
          leave();
        }
      }}
    >
      <b>Request changes</b>
      <textarea
        aria-label="What needs to change?"
        placeholder="What needs to change?"
        required
        rows={4}
        value={note}
        disabled={busy || own || !ownKnown}
        onChange={(event) => onNote(event.target.value)}
        autoFocus
      />
      {own ? (
        <p className="pull-review-note" data-tone="t2">
          <Lock size={14} aria-hidden />
          You can{"’"}t request changes on your own pull request.
        </p>
      ) : error ? (
        <p className="pull-review-note" data-tone="danger" role="alert">
          <TriangleAlert size={14} aria-hidden />
          {error}
        </p>
      ) : (
        <p className="pull-review-note">Your note is sent to the author.</p>
      )}
      <div className="pull-review-actions">
        <Button small disabled={busy} onClick={leave}>
          Cancel
        </Button>
        <Button
          small
          tone="primary"
          busy={busy}
          disabled={own || !ownKnown || !note.trim()}
          icon={error && !busy ? <RefreshCw size={14} aria-hidden /> : undefined}
          onClick={() => void send()}
        >
          {busy ? "Requesting…" : error ? "Try again" : "Request changes"}
        </Button>
      </div>
      {discarding ? (
        <ConfirmDialog title="Discard your note?" confirmLabel="Discard" tone="danger" cancelFocus onClose={() => setDiscarding(false)} onConfirm={async () => onCancel()}>
          <p>What you wrote is not sent.</p>
        </ConfirmDialog>
      ) : null}
    </section>
  );
}

/** What the card becomes once the review is in: an entry in the left column with the note. */
export function RequestedChangesEntry({ note }: { note: string }) {
  return (
    <section className="pull-card pull-review-done" aria-label="You requested changes">
      <b>You requested changes {"\u00b7"} just now</b>
      <span className="pull-review-note-text">{note}</span>
    </section>
  );
}
