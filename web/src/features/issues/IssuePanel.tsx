import { ExternalLink as ExternalIcon } from "lucide-react";
import { relativeTime } from "../../lib/format";
import { safeHref } from "../../lib/links";
import { useWaited } from "../../lib/motion";
import { useAsync } from "../../lib/useAsync";
import { ApiFailure } from "../../api/client";
import { routeHref } from "../../product/state/route";
import { Button } from "../../ui/controls";
import { issuesApi, useApiVersion } from "./api";
import { Body, Person } from "./IssueDetail";
import { IssueGlyph } from "./IssueGlyph";
import { isIssueKey, isTracker, stateWord, trackerName } from "./model";

/**
 * An issue as a tab of the thread's side panel, read only: the same text the Issues page shows, in the narrow column, so the
 * thread stays beside it. The tracker is where it is written; its state is changed on the issue's own page. The title, labels,
 * names and body are the tracker's text and are drawn as text.
 */
export function IssuePanel({ tracker, scope, issueKey }: { tracker: string; scope: string; issueKey: string }) {
  const version = useApiVersion();
  const read = useAsync(`issues:panel:${version}:${tracker}:${scope}:${issueKey}`, async (signal) => {
    if (!isTracker(tracker) || !isIssueKey(issueKey)) throw new ApiFailure("not_found", "No such tracker.", 404);
    return issuesApi().get({ tracker, scope, key: issueKey }, signal);
  });
  const waited = useWaited(read.status === "loading");
  const display = /^\d+$/.test(issueKey) ? `#${issueKey}` : issueKey;
  if (read.status === "loading") {
    return <div className="iss-panel" aria-busy>{waited ? <div className="iss-detail-skel" aria-hidden><span className="iss-bar" style={{ width: "60%", height: 16 }} /><span className="iss-bar" style={{ width: "90%", height: 12 }} /><span className="iss-bar" style={{ width: "75%", height: 12 }} /></div> : null}</div>;
  }
  if (read.status === "error") {
    return (
      <div className="iss-panel">
        <div className="iss-block" role="alert">
          {read.error.status === 404 ? <h2>{`No issue ${display} on ${isTracker(tracker) ? trackerName(tracker) : "that tracker"}.`}</h2> : <><h2>Couldn’t load this issue</h2><p>Nothing changed.</p></>}
          {read.error.status === 404 ? null : <div className="iss-block-actions"><Button tone="primary" onClick={() => void read.reload()}>Retry</Button></div>}
        </div>
      </div>
    );
  }
  const issue = read.data;
  const href = safeHref(issue.url);
  return (
    <div className="iss-panel">
      <header className="iss-panel-head">
        <span className="iss-quiet">{issue.display_key} · {trackerName(issue.tracker)} · {issue.scope}</span>
        <h2>{issue.title}</h2>
        <span className="iss-state"><IssueGlyph state={issue.state} />{stateWord(issue.state)}</span>
        <span className="iss-quiet">{issue.author ? issue.author.login : "Unknown author"} · opened {relativeTime(issue.created_at).toLowerCase()}</span>
      </header>
      <dl className="iss-panel-facts">
        <dt>Labels</dt><dd>{issue.labels.length === 0 ? <span className="iss-quiet">None</span> : issue.labels.map((label, at) => <span key={`${at}:${label.name}`} className="iss-label">{label.name}</span>)}</dd>
        <dt>Assignees</dt><dd>{issue.assignees.length === 0 ? <span className="iss-quiet">None</span> : issue.assignees.map((who) => <Person key={who.login} login={who.login} />)}</dd>
      </dl>
      <div className="iss-actions">
        {href ? <a className="btn btn-secondary" href={href} target="_blank" rel="noopener noreferrer"><ExternalIcon size={14} aria-hidden />Open on {trackerName(issue.tracker)}</a> : null}
        <a className="btn btn-secondary" href={routeHref({ name: "issues", tracker: issue.tracker, scope: issue.scope, key: issue.key })}>Open as a page</a>
      </div>
      <Body issue={issue} />
    </div>
  );
}
