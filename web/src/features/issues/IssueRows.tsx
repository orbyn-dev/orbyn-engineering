import type { IssueSummary } from "./contract";
import { compactAge } from "../../lib/format";
import { IssueGlyph } from "./IssueGlyph";
import { stateWord, trackerName } from "./model";

export function IssuesHead() {
  return (
    <div className="iss-head" aria-hidden>
      <span>Title and tracker</span>
      <span>Labels</span>
      <span>State</span>
      <span>Assignees</span>
      <span>Updated</span>
    </div>
  );
}

const initials = (login: string) => login.slice(0, 2).toUpperCase();

/** One quiet line of detail under the title; state as a word and a shape. Everything from the tracker is text. */
export function IssueRows({ issues, onOpen }: { issues: IssueSummary[]; onOpen(issue: IssueSummary): void }) {
  return (
    <ul className="iss-rows">
      {issues.map((issue) => (
        <li key={`${issue.tracker}:${issue.scope}:${issue.key}`} onClick={() => onOpen(issue)}>
          <span className="iss-cell-title">
            <button type="button" className="iss-open">{issue.title}</button>
            <span className="iss-sub">{issue.display_key} · {trackerName(issue.tracker)}{issue.author ? ` · opened by ${issue.author.login}` : ""}</span>
          </span>
          <span className="iss-labels">{issue.labels.slice(0, 3).map((label, at) => <span key={`${at}:${label.name}`} className="iss-label">{label.name}</span>)}</span>
          <span className="iss-state"><IssueGlyph state={issue.state} />{stateWord(issue.state)}</span>
          <span className="iss-people">
            {issue.assignees.length === 0 ? <span className="iss-quiet">None</span> : issue.assignees.slice(0, 3).map((who) => <span key={who.login} className="iss-avatar" title={who.login}>{initials(who.login)}</span>)}
          </span>
          <time className="iss-age" dateTime={issue.updated_at}>{compactAge(issue.updated_at)}</time>
        </li>
      ))}
    </ul>
  );
}

export function IssuesSkeleton() {
  return (
    <ul className="iss-skel" aria-hidden>
      {Array.from({ length: 6 }, (_, at) => (
        <li key={at}>
          <span className="iss-bar" style={{ width: `${55 + ((at * 13) % 30)}%`, height: 12 }} />
          <span className="iss-bar" style={{ width: 70, height: 10 }} />
          <span className="iss-bar" style={{ width: 48, height: 10 }} />
          <span className="iss-bar" style={{ width: 28, height: 18, borderRadius: 9 }} />
          <span className="iss-bar" style={{ width: 24, height: 10 }} />
        </li>
      ))}
    </ul>
  );
}
