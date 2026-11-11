import { useMemo, useState } from "react";
import { relativeTime } from "../../../lib/format";
import { pullChanged } from "../model/pullTopics";
import { useAsync } from "../../../lib/useAsync";
import { ErrorState, Loading } from "../../../product/ui/states";
import { pullsApi } from "../api";

/**
 * The pull request's activity: comments, reviews, commits and the rest. It is paginated on the forge, so it is read
 * only when something shows it (the Conversation tab); until then the tab's count is the comments' count.
 */
export function useTimelineEvents(projectId: string, number: number, enabled: boolean) {
  return useAsync(
    `pull-timeline:${projectId}:${number}:${enabled}`,
    (signal) => (enabled ? pullsApi.timeline(projectId, number, signal) : Promise.resolve(null)),
    { keep: true, refreshOn: pullChanged(projectId, number) },
  );
}

export type TimelineRead = ReturnType<typeof useAsync<import("../../../api/types").PullTimelineEvent[]>>;

export function Timeline({ projectId, number }: { projectId: string; number: number }) {
  return <TimelineList events={useTimelineEvents(projectId, number, true) as TimelineRead} />;
}

export function TimelineList({ events }: { events: TimelineRead }) {
  const [kind, setKind] = useState("all");
  const kinds = useMemo(() => [...new Set((events.data ?? []).map((event) => event.kind))].sort(), [events.data]);
  if (events.status === "loading" || (events.status === "ready" && !events.data)) return <Loading label="Loading timeline" rows={4} />;
  if (events.status === "error") {
    return <ErrorState compact title="Couldn't load the timeline" failure={events.error} onRetry={events.reload} />;
  }
  if (!events.data || events.data.length === 0) return <p className="muted">No activity yet.</p>;
  return (
    <>
      <select aria-label="Filter activity" value={kind} onChange={(event) => setKind(event.target.value)}>
        <option value="all">All activity</option>
        {kinds.map((entry) => (
          <option key={entry} value={entry}>
            {entry}
          </option>
        ))}
      </select>
      <ol className="pull-timeline">
        {events.data
          .filter((event) => kind === "all" || event.kind === kind)
          .map((event, at) => (
            <li key={event.id || `${event.kind}:${at}`}>
              <span className="pull-actor">{event.actor ?? "Someone"}</span> {event.text ?? event.kind}
              {event.created_at ? <time>{relativeTime(event.created_at)}</time> : null}
            </li>
          ))}
      </ol>
    </>
  );
}
