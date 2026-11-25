import { useRef } from "react";
import type { Thread } from "../../../api/types";
import { chipLabel, type ReviewChip } from "../../../product/composer/composerContext";
import { batchSummary, groupByFile, reviewMessage } from "../../../product/composer/reviewBatch";
import { useUi } from "../../../product/state/ui";
import { ConfirmDialog } from "../../../ui/Dialog";
import { sendReviewDrafts } from "./sendReview";

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/** The confirm for Send all to engine: it shows the one message that will go, and sends nothing until Send. */
export function SendAllDialog({ thread, drafts, onClose }: { thread: Thread; drafts: readonly ReviewChip[]; onClose(): void }) {
  const { comments, files } = batchSummary(drafts);
  const queued = thread.active_run !== null;
  // The in-flight guard in sendReviewDrafts is the real one; this keeps a second press from even reaching it.
  const pressed = useRef(false);
  return (
    <ConfirmDialog
      title={`Send ${plural(comments, "comment", "comments")} to the engine?`}
      confirmLabel={`Send ${plural(comments, "comment", "comments")}`}
      className="review-send"
      onClose={onClose}
      onConfirm={async () => {
        if (pressed.current) return;
        pressed.current = true;
        try {
          if (await sendReviewDrafts(thread, drafts)) {
            useUi.getState().pushToast(`${plural(comments, "comment", "comments")} sent as one message`);
          }
        } finally {
          pressed.current = false;
        }
      }}
    >
      <p>{queued
        ? "They go together as one message. The agent is working, so it is queued and read with its next turn."
        : "They go together as one message, so the agent can fix everything in a single pass and reply once."}</p>
      <p className="review-send-count"><strong>{plural(comments, "comment", "comments")} in {plural(files, "file", "files")}</strong></p>
      <ul className="review-send-list">
        {groupByFile(drafts).map((group) => <li key={group.path}>
          <span className="review-path">{group.path}</span> <span className="t3">· {plural(group.drafts.length, "comment", "comments")}</span>
          <ul>{group.drafts.map((draft) => <li key={draft.id}><code>{chipLabel(draft)}</code> {draft.comment}</li>)}</ul>
        </li>)}
      </ul>
      <details className="review-send-exact">
        <summary>The exact message the engine receives</summary>
        <pre>{reviewMessage(drafts, thread.worktree?.branch ?? null)}</pre>
        <p className="t3">Each comment is sent with the lines it is on.</p>
      </details>
    </ConfirmDialog>
  );
}
