import { IssueGlyph } from "./IssueGlyph";
import { useThreadIssues } from "./links";
import { threadMarker } from "./model";

/**
 * The small marker before a thread row's time: the ring of its issue, and the count when there are several. The key is in
 * the accessible name and the tooltip, not in the row. A thread with no linked issue, and a service that reports no issues, draw nothing.
 */
export function IssueMark({ threadId }: { threadId: string }) {
  const { issues } = useThreadIssues(threadId);
  const mark = threadMarker(issues);
  if (!mark) return null;
  const state = { category: mark.closed ? ("closed" as const) : ("open" as const), name: mark.closed ? "Closed" : "Open" };
  return (
    <span className="iss-mark" role="img" aria-label={mark.label} title={mark.label}>
      <IssueGlyph state={state} size={12} />
      {mark.count > 1 ? <span className="iss-mark-count">{mark.count}</span> : null}
    </span>
  );
}
