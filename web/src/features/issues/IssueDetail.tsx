import { ArrowLeft, ExternalLink as ExternalIcon, GitMerge, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft, Plus } from "lucide-react";
import { useRef, useState } from "react";
import { relativeTime, compactAge } from "../../lib/format";
import { safeHref } from "../../lib/links";
import { useWaited } from "../../lib/motion";
import { useAsync } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { routeHref } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { PageBar } from "../../product/ui/PageBar";
import { Button } from "../../ui/controls";
import { ApiFailure } from "../../api/client";
import { Markdown } from "../content";
import { issuesApi, useApiVersion, useEditOffered } from "./api";
import { capText, closedWords, isIssueKey, isTracker, linkedChecksWord, pullWord, stateWord, trackerName } from "./model";
import type { IssueDetail as Detail, LinkedPullRequest } from "./contract";
import { CommentBox } from "./IssueComment";
import { PickerEditor, useDescriptionEditor, type EditProps, type IssueWriter } from "./IssueEdit";
import { IssueGlyph } from "./IssueGlyph";
import { Person } from "./IssuePerson";
import { ReactionBar } from "./IssueReactions";
import { IssueStateButton } from "./IssueStateMenu";
import { LinkedThreadsChip } from "./LinkedThreadsChip";
import { LinkThreadDialog } from "./LinkThreadDialog";
import { reloadIssueLinks, useIssueThreads } from "./links";
import { StartThreadDialog } from "./StartThreadDialog";

function LinkedThreads({ issue, onStart, link, onLinked }: { issue: Detail; onStart(): void; link(threadId: string): Promise<boolean>; onLinked(): void }) {
  const { threadIds, loaded, failed, retry } = useIssueThreads(issue);
  const threads = useWorkspace((state) => state.threads);
  const [linking, setLinking] = useState(false);
  const known = threadIds.flatMap((id) => (threads[id] ? [threads[id]] : []));
  return (
    <section className="iss-card" role="group" aria-label="Linked threads">
      <h2>Linked threads</h2>
      {known.length === 0 && failed ? (
        // The thread links are their own read: only this card says it failed, and the facts and the pull requests stay.
        <div className="iss-start-fail" role="alert">
          <span className="iss-quiet">Couldn’t read the linked threads. Nothing changed.</span>
          <Button onClick={() => void retry()}>Retry</Button>
        </div>
      ) : known.length === 0 ? <p className="iss-quiet">{loaded ? "No thread is linked to this issue yet." : ""}</p> : (
        <ul className="iss-threads">
          {known.map((thread) => (
            <li key={thread.id}>
              <a href={routeHref({ name: "thread", id: thread.id })}>
                <b>{thread.title}</b>
                <span className="iss-quiet">{thread.active_run ? "Running" : compactAge(thread.updated_at)}</span>
                <span className="iss-quiet">{[thread.engine, thread.worktree?.branch].filter(Boolean).join(" · ")}</span>
              </a>
            </li>
          ))}
        </ul>
      )}
      {failed && known.length > 0 ? (
        // The read after a link failed while the card still lists what it knew: the header counts the linked threads from
        // the issue's own read and can already show the new one, so the card says it is behind and Retry reads it again
        // (the link write itself already succeeded and is not repeated).
        <div className="iss-start-fail" role="alert">
          <span className="iss-quiet">Couldn’t refresh the linked threads. The list may be out of date.</span>
          <Button onClick={() => void retry()}>Retry</Button>
        </div>
      ) : null}
      <div className="iss-thread-acts">
        <Button icon={<Plus size={14} aria-hidden />} onClick={onStart}>Start a thread from this issue</Button>
        <Button onClick={() => setLinking(true)}>Link to a thread…</Button>
      </div>
      {linking ? <LinkThreadDialog issue={issue} link={link} onClose={() => setLinking(false)} onLinked={onLinked} /> : null}
    </section>
  );
}

const PULL_ICON = { merged: GitMerge, closed: GitPullRequestClosed, draft: GitPullRequestDraft, open: GitPullRequest } as const;

/**
 * One pull request the tracker reports against the issue. Its words are the tracker's text, capped; only a plain https address is
 * opened. Its checks are the same counts the pull request list carries, in that list's words; absent checks are unknown and draw
 * nothing, never "No checks".
 */
function PullRow({ pull }: { pull: LinkedPullRequest }) {
  const href = safeHref(pull.url);
  const word = pullWord(pull);
  const checks = linkedChecksWord(pull.checks);
  const Icon = PULL_ICON[pull.state === "open" && pull.is_draft ? "draft" : pull.state];
  const body = (
    <>
      <span className="iss-pr-line">
        <Icon size={14} aria-hidden />
        <b>{capText(pull.title, 200)}</b>
        {word ? <span className="iss-pr-state">{word}</span> : null}
      </span>
      <span className="iss-pr-sub">
        <span>#{pull.number}</span>
        <span>{capText(pull.repository, 80)}</span>
        {pull.closes_issue ? <span>Closes this issue</span> : null}
        {checks ? (
          <span className="iss-pr-checks" data-tone={checks.tone}>
            <span className="sr-only">Checks </span>
            {checks.label}
          </span>
        ) : null}
      </span>
    </>
  );
  return <li>{href ? <a href={href} target="_blank" rel="noopener noreferrer">{body}</a> : <span className="iss-pr-plain">{body}</span>}</li>;
}

/** What the tracker reports of its own accord, in its order. A link's checks are drawn when the tracker reported them; unknown draws nothing. */
function LinkedPullRequests({ pulls }: { pulls: LinkedPullRequest[] }) {
  // Shown only when the tracker reports a pull request: an empty card on every issue says nothing.
  if (pulls.length === 0) return null;
  return (
    <section className="iss-card" role="group" aria-label="Linked pull requests">
      <h2>{`Linked pull requests · ${pulls.length}`}</h2>
      <ul className="iss-prs">{pulls.map((pull, at) => <PullRow key={`${at}:${pull.repository}#${pull.number}`} pull={pull} />)}</ul>
    </section>
  );
}

function Entry({ issue }: { issue: Detail }) {
  return (
    <article className="iss-entry">
      <header>{issue.author ? <Person login={issue.author.login} /> : <span className="iss-quiet">Unknown author</span>}<span className="iss-quiet">opened {relativeTime(issue.created_at).toLowerCase()}</span></header>
      {/* Escaped Markdown through the app's safe renderer: no HTML, and remote images stay links. */}
      <div className="iss-body"><Markdown text={issue.body} remoteImages="link" /></div>
    </article>
  );
}

/** The post with the description editor and the reaction bar: the header's pencil, and the editor in place of the text while it is open. */
function EditableEntry({ issue, edit }: { issue: Detail; edit: EditProps }) {
  const { pencil, editor, editing } = useDescriptionEditor(edit);
  return (
    <article className="iss-entry">
      <header>{issue.author ? <Person login={issue.author.login} /> : <span className="iss-quiet">Unknown author</span>}<span className="iss-quiet">opened {relativeTime(issue.created_at).toLowerCase()}</span>{pencil}</header>
      {editing ? editor : <div className="iss-body"><Markdown text={issue.body} remoteImages="link" /></div>}
      <ReactionBar issue={issue} write={edit.write} writing={edit.writing} />
    </article>
  );
}

/** `page` is the issue as a page: it adds the description pencil (Planned until the service offers editing) and the linked pull requests after the description; the panel tab does not. */
export function Body({ issue, page = false, edit }: { issue: Detail; page?: boolean; edit?: EditProps }) {
  return (
    <div className="iss-main">
      {page && edit ? <EditableEntry issue={issue} edit={edit} /> : <Entry issue={issue} />}
      {page ? <LinkedPullRequests pulls={issue.linked_pull_requests} /> : null}
      {issue.comments.map((comment) => (
        <article key={comment.id} className="iss-entry">
          <header>{comment.author ? <Person login={comment.author.login} /> : <span className="iss-quiet">Unknown author</span>}<span className="iss-quiet">{relativeTime(comment.created_at).toLowerCase()}</span></header>
          <div className="iss-body"><Markdown text={comment.body} remoteImages="link" /></div>
        </article>
      ))}
      {/* The page adds the comment box; the panel tab stays read only. */}
      {page && edit ? <CommentBox issue={issue} write={edit.write} writing={edit.writing} /> : null}
    </div>
  );
}

/** The issue's facts. `host` is the host the tracker reports for its connection, shown as text only when there is one. */
function Facts({ issue, edit, host }: { issue: Detail; edit: EditProps; host?: string | null }) {
  return (
    <section className="iss-card" role="group" aria-label="Facts">
      <dl>
        <dt>State</dt><dd><IssueStateButton issue={issue} write={edit.write} writing={edit.writing} /></dd>
        <dt>Tracker</dt><dd>{trackerName(issue.tracker)}</dd>
        {host ? <><dt>Host</dt><dd>{host}</dd></> : null}
        <dt>Author</dt><dd>{issue.author?.login ?? "Unknown"}</dd>
        <dt>Assignees</dt><dd>{issue.assignees.length === 0 ? <span className="iss-quiet">None</span> : issue.assignees.map((who) => <Person key={who.login} login={who.login} />)}<PickerEditor kind="assignees" {...edit} /></dd>
        <dt>Labels</dt><dd>{issue.labels.length === 0 ? <span className="iss-quiet">None</span> : issue.labels.map((label, at) => <span key={`${at}:${label.name}`} className="iss-label">{label.name}</span>)}<PickerEditor kind="labels" {...edit} /></dd>
        <dt>Opened</dt><dd>{relativeTime(issue.created_at).toLowerCase()}</dd>
        {closedWords(issue) ? <><dt>Closed</dt><dd>{relativeTime(issue.closed_at!).toLowerCase()}</dd></> : null}
        <dt>Updated</dt><dd>{relativeTime(issue.updated_at).toLowerCase()}</dd>
      </dl>
    </section>
  );
}

/** One issue as a page. Start a thread is a click; the state, description, labels and assignees are written only when the service offers them, one write at a time. */
export function IssueDetail({ tracker, scope, issueKey, host }: { tracker: string; scope: string; issueKey: string; host?: string | null }) {
  const version = useApiVersion();
  const phone = useLayoutWidth() === "phone";
  const [starting, setStarting] = useState(false);
  const offered = useEditOffered();
  const read = useAsync(`issues:detail:${version}:${tracker}:${scope}:${issueKey}`, async (signal) => {
    if (!isTracker(tracker) || !isIssueKey(issueKey)) throw new ApiFailure("not_found", "No such tracker.", 404);
    return issuesApi().get({ tracker, scope, key: issueKey }, signal);
  });
  const waited = useWaited(read.status === "loading");
  // One write at a time on this page, whichever control makes it: a second click, or another control, while one is in flight is
  // refused and calls nothing. The answer is the updated issue and replaces what is shown; the fields a write leaves out are kept.
  const [writing, setWriting] = useState(false);
  const locked = useRef(false);
  const shown = useRef(read.data);
  shown.current = read.data;
  const write: IssueWriter = async (call) => {
    if (locked.current) return null;
    locked.current = true;
    setWriting(true);
    try {
      const next = await call();
      // States and the reaction counts come from some answers only (a plain detail read carries no reactions), so a write must not
      // empty what the page already showed: the bar would blink off after an unrelated edit.
      read.setData({ ...next, states: next.states ?? shown.current?.states, reactions: next.reactions ?? shown.current?.reactions });
      void reloadIssueLinks(version);
      return next;
    } finally {
      locked.current = false;
      setWriting(false);
    }
  };
  // The card's link is a write on this page too: it takes the same guard, then reads the links again from a read that starts
  // after the write, so the card lists the thread the tracker now holds rather than an answer read before it.
  const linkThread = async (threadId: string): Promise<boolean> => {
    const target = read.data;
    if (!target || locked.current) return false;
    locked.current = true;
    setWriting(true);
    try {
      await issuesApi().link(threadId, target);
      await reloadIssueLinks(version, { afterWrite: true });
      return true;
    } finally {
      locked.current = false;
      setWriting(false);
    }
  };
  const back = <a className="iss-back" href={routeHref({ name: "issues" })}><ArrowLeft size={14} aria-hidden />Back to Issues</a>;
  const issue = read.data;
  const href = issue ? safeHref(issue.url) : null;
  const display = issue?.display_key ?? (/^\d+$/.test(issueKey) ? `#${issueKey}` : issueKey);
  return (
    <>
      <PageBar back={{ name: "issues" }} actions={issue && issue.linked_threads.length > 0 ? <LinkedThreadsChip issue={issue} /> : undefined}>
        <h1 tabIndex={-1} data-page-title>{issue?.title ?? "Issue"}</h1>
        {issue ? <span className="page-sub"><span className="page-sub-item">{issue.display_key} · {trackerName(issue.tracker)} · {issue.scope}</span><span className="page-sub-item iss-state"><IssueGlyph state={issue.state} />{stateWord(issue.state)}</span></span> : null}
      </PageBar>
      <div className="iss-detail">
        {phone ? null : back}
        {read.status === "loading" ? (
          waited ? <div className="iss-detail-skel" aria-hidden><span className="iss-bar" style={{ width: "50%", height: 16 }} /><span className="iss-bar" style={{ width: "90%", height: 12 }} /><span className="iss-bar" style={{ width: "80%", height: 12 }} /></div> : null
        ) : read.status === "error" ? (
          <div className="iss-block" role="alert">
            {read.error.status === 404 ? <h2>{`No issue ${display} on ${isTracker(tracker) ? trackerName(tracker) : "that tracker"}.`}</h2> : <><h2>Couldn’t load this issue</h2><p>Nothing changed.</p></>}
            {read.error.status === 404 ? null : <div className="iss-block-actions"><Button tone="primary" onClick={() => void read.reload()}>Retry</Button></div>}
          </div>
        ) : (
          <div className="iss-cols">
            <div className="iss-left">
              <div className="iss-actions">
                {href ? <a className="btn btn-secondary" href={href} target="_blank" rel="noopener noreferrer"><ExternalIcon size={14} aria-hidden />Open on {trackerName(read.data.tracker)}</a> : null}
                <Button tone="primary" onClick={() => setStarting(true)}>Start a thread</Button>
              </div>
              <Body issue={read.data} page edit={{ issue: read.data, offered, write, writing }} />
            </div>
            <aside className="iss-right">
              <Facts issue={read.data} edit={{ issue: read.data, offered, write, writing }} host={host} />
              <LinkedThreads issue={read.data} onStart={() => setStarting(true)} link={linkThread} onLinked={() => { void read.refreshQuietly(); }} />
            </aside>
          </div>
        )}
      </div>
      {starting && issue ? <StartThreadDialog issue={issue} onClose={() => setStarting(false)} /> : null}
    </>
  );
}
