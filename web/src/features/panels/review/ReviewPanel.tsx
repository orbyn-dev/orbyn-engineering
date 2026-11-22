import { FileDiff } from "lucide-react";
import { useState } from "react";
import { MAX_PROMPT_CHARS, charactersOver, chipLabel, promptWithChips, type ReviewChip } from "../../../product/composer/composerContext";
import { reviewDrafts } from "../../../product/composer/reviewBatch";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { Empty } from "../../../product/ui/states";
import { ForgeComments } from "./ForgeComments";
import { SendAllDialog } from "./SendAllDialog";
import { forgeGroups, useForgeComments } from "./useForgeComments";

const NO_CHIPS: never[] = [];

/** One comment: where it is, its state and text (always as text), and what can be done with it. */
function Item({ threadId, chip, sent, waiting }: { threadId: string; chip: ReviewChip; sent: boolean; waiting: boolean }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const save = () => {
    if (text === null || !text.trim()) return;
    const next = { ...chip, comment: text.trim() };
    const have = useComposerChips.getState().chips[threadId] ?? [];
    const all = have.map((current) => current.id === chip.id ? next : current);
    if (charactersOver(promptWithChips(useUi.getState().drafts[threadId] ?? "", all)) > 0) {
      setError(`This comment would make your message too long (the limit is ${MAX_PROMPT_CHARS.toLocaleString()} characters). Shorten it.`);
      return;
    }
    useComposerChips.getState().update(threadId, next);
    setText(null);
    setError(null);
  };
  return <li className="review-item" data-state={sent ? "open" : "draft"}>
    <div className="review-item-head">
      <button type="button" className="review-path" title={chip.path} aria-label={`Open ${chipLabel(chip)}`}
        onClick={() => useUi.getState().revealFile(chip.path, chip.line, { threadId, cwd: chip.sourceCwd ?? cwd ?? "" })}>
        <bdi>{chipLabel(chip)}</bdi>
      </button>
      {sent ? <span className="review-badge">{waiting ? "Waiting for the agent" : "Sent"}</span>
        : <><span className="review-badge">Not sent</span><span className="review-badge">Draft</span></>}
      {chip.stale ? <span className="review-badge" data-tone="warn" title="The file was edited: this text may have changed or moved">Out of date</span> : null}
    </div>
    {text === null ? <p className="review-comment">{chip.comment}</p> : <>
      <label className="sr-only" htmlFor={`review-edit-${chip.id}`}>Comment</label>
      <textarea id={`review-edit-${chip.id}`} aria-label="Comment" maxLength={20000} value={text} autoFocus onChange={(event) => setText(event.target.value)} />
      {error ? <p role="alert" className="field-error">{error}</p> : null}
    </>}
    {sent ? null : <div className="review-actions">
      {text === null ? <>
        <Button small tone="quiet" aria-label="Edit comment" onClick={() => setText(chip.comment)}>Edit</Button>
        <Button small tone="quiet" aria-label="Delete comment" onClick={() => useComposerChips.getState().remove(threadId, chip.id)}>Delete</Button>
      </> : <>
        <Button small onClick={() => { setText(null); setError(null); }}>Cancel</Button>
        <Button small tone="primary" disabled={!text.trim()} onClick={save}>Save</Button>
      </>}
    </div>}
  </li>;
}

/**
 * Every line comment on this thread's changes. Drafts are the review chips waiting above the composer (file and diff
 * comments alike); Open ones were sent this session. Send all to engine sends the drafts as one message. The linked
 * pull request's forge threads follow under their own groups.
 */
export function ReviewPanel({ threadId }: { threadId: string }) {
  const thread = useWorkspace((state) => state.threads[threadId]);
  const forgeRead = useForgeComments(threadId, thread?.project_id ?? "");
  const drafts = reviewDrafts(useComposerChips((state) => state.chips[threadId] ?? NO_CHIPS));
  const open = reviewDrafts(useComposerChips((state) => state.sent[threadId] ?? NO_CHIPS));
  const [file, setFile] = useState("");
  const [confirming, setConfirming] = useState<ReviewChip[] | null>(null);
  const forge = forgeGroups(forgeRead.data);
  if (!thread) {
    return drafts.length === 0 && open.length === 0
      ? <Empty title="No comments yet"><p className="t3">Click a line number in Changes to comment.</p></Empty>
      : null;
  }
  if (drafts.length === 0 && open.length === 0 && forge.open.length === 0 && forge.resolved.length === 0
    && forgeRead.status === "ready") {
    return <Empty title="No comments yet"><p className="t3">Click a line number in Changes to comment.</p></Empty>;
  }
  const forgeFiles = [...forge.open, ...forge.resolved].flatMap((item) => item.path === null ? [] : [item.path]);
  const files = [...new Set([...drafts, ...open].map((chip) => chip.path).concat(forgeFiles))];
  const shown = (list: ReviewChip[]) => list.filter((chip) => file === "" || chip.path === file);
  return <div className="review-panel">
    {files.length > 1 ? <label className="review-filter"><span className="sr-only">Filter by file</span>
      <select aria-label="Filter by file" value={file} onChange={(event) => setFile(event.target.value)}>
        <option value="">All files</option>
        {files.map((path) => <option key={path} value={path}>{path}</option>)}
      </select></label> : null}
    {drafts.length > 0 ? <section aria-label="Drafts">
      <div className="review-group-head">
        <h4>Drafts · {drafts.length}</h4>
        <Button small tone="primary" icon={<FileDiff size={13} aria-hidden />} onClick={() => setConfirming(drafts)}>Send all to engine</Button>
      </div>
      <ul className="review-list">{shown(drafts).map((chip) => <Item key={chip.id} {...{ threadId, chip }} sent={false} waiting={false} />)}</ul>
    </section> : null}
    {open.length > 0 ? <section aria-label="Open">
      <div className="review-group-head"><h4>Open · {open.length}</h4></div>
      <ul className="review-list">{shown(open).map((chip) => <Item key={chip.id} {...{ threadId, chip }} sent waiting={thread.active_run !== null} />)}</ul>
    </section> : null}
    <ForgeComments threadId={threadId} cwd={thread.cwd} file={file} read={forgeRead} onRetry={forgeRead.reload} />
    {confirming ? <SendAllDialog thread={thread} drafts={confirming} onClose={() => setConfirming(null)} /> : null}
  </div>;
}
