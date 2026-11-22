import { useState } from "react";
import type { PullThread } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { forgeGroups, type ForgeRead } from "./useForgeComments";

/** How much of a comment body the tab shows before the rest moves behind Show more. */
const BODY_LIMIT = 400;

function ThreadItem({ thread, threadId, cwd }: { thread: PullThread; threadId: string; cwd: string }) {
  const [expanded, setExpanded] = useState(false);
  const first = thread.comments[0];
  if (!first) return null;
  const path = thread.path;
  const line = thread.line;
  const location = path === null ? null : line === null ? path : `${path}:${line}`;
  const jump = path !== null && line !== null;
  const long = first.body.length > BODY_LIMIT;
  const body = long && !expanded ? first.body.slice(0, BODY_LIMIT) : first.body;
  return <li className="review-item" data-state={thread.resolved ? "resolved" : "open"}>
    <div className="review-item-head">
      {location === null ? null : jump
        ? <button type="button" className="review-path" title={location} aria-label={`Open ${location}`}
          onClick={() => useUi.getState().revealFile(path, line, { threadId, cwd })}>
          <bdi>{location}</bdi>
        </button>
        : <span className="review-path">{location}</span>}
      <span className="review-badge">{first.author ?? "Unknown"}</span>
      {thread.comments.length > 1 ? <span className="review-badge">{thread.comments.length} comments</span> : null}
    </div>
    <p className="review-comment">{body}</p>
    {long ? <div className="review-actions">
      <Button small tone="quiet" onClick={() => setExpanded((open) => !open)}>{expanded ? "Show less" : "Show more"}</Button>
    </div> : null}
  </li>;
}

/**
 * The linked pull requests' forge review threads: unresolved ones under On the pull request, and
 * resolved ones behind a closed disclosure. Comment bodies are always plain text.
 */
export function ForgeComments({ threadId, cwd, file, read, onRetry }: {
  threadId: string;
  cwd: string;
  file: string;
  read: ForgeRead;
  onRetry(): void;
}) {
  const [resolvedShown, setResolvedShown] = useState(false);
  const groups = forgeGroups(read.data);
  const matching = (threads: PullThread[]) => threads.filter((thread) => file === "" || thread.path === file);
  return <>
    {read.status === "error" ? <div className="review-forge-error" role="status">
      <p>Pull request comments could not be read.</p>
      <Button small tone="quiet" onClick={onRetry}>Try again</Button>
    </div> : null}
    {groups.open.length > 0 ? <section aria-label="On the pull request">
      <div className="review-group-head"><h4>On the pull request · {groups.open.length}</h4></div>
      <ul className="review-list">{matching(groups.open).map((thread) => <ThreadItem key={thread.id} {...{ thread, threadId, cwd }} />)}</ul>
    </section> : null}
    {groups.resolved.length > 0 ? <section aria-label="Resolved">
      <div className="review-group-head">
        <h4><button type="button" className="review-disclosure" aria-expanded={resolvedShown}
          onClick={() => setResolvedShown((shown) => !shown)}>Resolved · {groups.resolved.length}</button></h4>
      </div>
      {resolvedShown ? <ul className="review-list">{matching(groups.resolved).map((thread) => <ThreadItem key={thread.id} {...{ thread, threadId, cwd }} />)}</ul> : null}
    </section> : null}
  </>;
}
