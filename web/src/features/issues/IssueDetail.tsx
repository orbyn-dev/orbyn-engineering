import { ArrowLeft, ExternalLink as ExternalIcon, Plus } from "lucide-react";
import { useState } from "react";
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
import { issuesApi, useApiVersion } from "./api";
import { isIssueKey, isTracker, stateWord, trackerName } from "./model";
import type { IssueDetail as Detail } from "./contract";
import { IssueGlyph } from "./IssueGlyph";
import { IssueStateButton } from "./IssueStateMenu";
import { reloadIssueLinks, useIssueThreads } from "./links";
import { StartThreadDialog } from "./StartThreadDialog";

export function Person({ login }: { login: string }) {
  return <span className="iss-person"><span className="iss-avatar" aria-hidden>{login.slice(0, 2).toUpperCase()}</span>{login}</span>;
}

function LinkedThreads({ issue, onStart }: { issue: Detail; onStart(): void }) {
  const { threadIds, loaded } = useIssueThreads(issue);
  const threads = useWorkspace((state) => state.threads);
  const known = threadIds.flatMap((id) => (threads[id] ? [threads[id]] : []));
  return (
    <section className="iss-card" role="group" aria-label="Linked threads">
      <h2>Linked threads</h2>
      {known.length === 0 ? <p className="iss-quiet">{loaded ? "No thread is linked to this issue yet." : ""}</p> : (
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
      <Button icon={<Plus size={14} aria-hidden />} onClick={onStart}>Start a thread from this issue</Button>
    </section>
  );
}

export function Body({ issue }: { issue: Detail }) {
  return (
    <div className="iss-main">
      <article className="iss-entry">
        <header>{issue.author ? <Person login={issue.author.login} /> : <span className="iss-quiet">Unknown author</span>}<span className="iss-quiet">opened {relativeTime(issue.created_at).toLowerCase()}</span></header>
        {/* Escaped Markdown through the app's safe renderer: no HTML, and remote images stay links. */}
        <div className="iss-body"><Markdown text={issue.body} remoteImages="link" /></div>
      </article>
      {issue.comments.map((comment) => (
        <article key={comment.id} className="iss-entry">
          <header>{comment.author ? <Person login={comment.author.login} /> : <span className="iss-quiet">Unknown author</span>}<span className="iss-quiet">{relativeTime(comment.created_at).toLowerCase()}</span></header>
          <div className="iss-body"><Markdown text={comment.body} remoteImages="link" /></div>
        </article>
      ))}
    </div>
  );
}

function Facts({ issue, onChanged }: { issue: Detail; onChanged(): Promise<unknown> | void }) {
  return (
    <section className="iss-card" role="group" aria-label="Facts">
      <dl>
        <dt>State</dt><dd><IssueStateButton issue={issue} onChanged={onChanged} /></dd>
        <dt>Tracker</dt><dd>{trackerName(issue.tracker)}</dd>
        <dt>Author</dt><dd>{issue.author?.login ?? "Unknown"}</dd>
        <dt>Assignees</dt><dd>{issue.assignees.length === 0 ? <span className="iss-quiet">None</span> : issue.assignees.map((who) => <Person key={who.login} login={who.login} />)}</dd>
        <dt>Labels</dt><dd>{issue.labels.length === 0 ? <span className="iss-quiet">None</span> : issue.labels.map((label, at) => <span key={`${at}:${label.name}`} className="iss-label">{label.name}</span>)}</dd>
        <dt>Updated</dt><dd>{relativeTime(issue.updated_at).toLowerCase()}</dd>
      </dl>
    </section>
  );
}

/** One issue as a page, read only: the tracker is where it is written. The one write is Start a thread, from a click. */
export function IssueDetail({ tracker, issueKey }: { tracker: string; issueKey: string }) {
  const version = useApiVersion();
  const phone = useLayoutWidth() === "phone";
  const [starting, setStarting] = useState(false);
  const read = useAsync(`issues:detail:${version}:${tracker}:${issueKey}`, async (signal) => {
    if (!isTracker(tracker) || !isIssueKey(issueKey)) throw new ApiFailure("not_found", "No such tracker.", 404);
    return issuesApi().get(tracker, issueKey, signal);
  });
  const waited = useWaited(read.status === "loading");
  const back = <a className="iss-back" href={routeHref({ name: "issues" })}><ArrowLeft size={14} aria-hidden />Back to Issues</a>;
  const issue = read.data;
  const href = issue ? safeHref(issue.url) : null;
  const display = issue?.display_key ?? (/^\d+$/.test(issueKey) ? `#${issueKey}` : issueKey);
  return (
    <>
      <PageBar back={{ name: "issues" }}>
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
              <Body issue={read.data} />
            </div>
            <aside className="iss-right">
              <Facts issue={read.data} onChanged={async () => { await Promise.all([read.reload(), reloadIssueLinks(version)]); }} />
              <LinkedThreads issue={read.data} onStart={() => setStarting(true)} />
            </aside>
          </div>
        )}
      </div>
      {starting && issue ? <StartThreadDialog issue={issue} onClose={() => setStarting(false)} /> : null}
    </>
  );
}
