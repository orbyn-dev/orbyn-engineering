import { Lock, MessageSquare, RefreshCw, TriangleAlert } from "lucide-react";
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
  forge,
  onCancel,
  onDone,
}: {
  projectId: string;
  number: number;
  /** The person opened this pull request, which a forge does not let them review. */
  own: boolean;
  forge: string;
  onCancel(): void;
  onDone(note: string): void;
}) {
  const [note, setNote] = useState("");
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
      setError(known.code === "unsupported" ? `${forge} doesn’t provide that here.` : "Couldn’t submit the review. Nothing was sent. Try again.");
      setBusy(false);
    }
  };
  const leave = () => (note.trim() ? setDiscarding(true) : onCancel());
  return (
    <section className="pull-card pull-review" aria-label="Request changes">
      <b>Request changes</b>
      <textarea
        aria-label="What needs to change?"
        placeholder="What needs to change?"
        required
        rows={4}
        value={note}
        disabled={busy || own}
        onChange={(event) => setNote(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            leave();
          }
        }}
        autoFocus
      />
      {own ? (
        <p className="pull-review-note">
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
          disabled={own || !note.trim()}
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

/** What the card becomes once the review is in: a timeline entry with the note. */
export function RequestedChangesEntry({ note }: { note: string }) {
  return (
    <section className="pull-card pull-review pull-review-done" aria-label="You requested changes">
      <span className="pull-review-head">
        <MessageSquare size={14} aria-hidden />
        <b>You requested changes</b> {"·"} just now
      </span>
      <p>{note}</p>
    </section>
  );
}
