import { RefreshCw } from "lucide-react";
import "./issues.css";
import { useEffect, useMemo, useRef, useState } from "react";
import { relativeTime } from "../../lib/format";
import { useWaited } from "../../lib/motion";
import { useAsync } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { navigate, useRoute } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { PageBar } from "../../product/ui/PageBar";
import { IconButton } from "../../ui/controls";
import { issuesApi, useApiVersion, useTrackers } from "./api";
import type { IssueQuery, IssueSummary, TrackerKind, TrackerRow } from "./contract";
import { IssueDetail } from "./IssueDetail";
import { IssueRows, IssuesHead, IssuesSkeleton } from "./IssueRows";
import { NoIssues, NoMatch, NotConnected, NotReported, Unreachable } from "./IssueStates";
import { IssuesToolbar } from "./IssuesToolbar";
import { isTracker, labelCounts, noQuery, trackerName } from "./model";

const KEY = "orbyn.issues.tracker";
/** The page remembers the last tracker. Storage can be blocked or empty; the page then starts on every tracker. */
function remembered(): TrackerKind | null {
  try {
    const value = window.localStorage.getItem(KEY);
    return value && isTracker(value) ? value : null;
  } catch { return null; }
}
function remember(tracker: TrackerKind | null): void {
  try {
    if (tracker) window.localStorage.setItem(KEY, tracker);
    else window.localStorage.removeItem(KEY);
  } catch { /* the choice just is not kept */ }
}

const sentence = (parts: string[]) => parts.join(" ");

/** What a query cannot find, in words: "Nothing in open issues matches “keyboard” with the label perf." */
function noMatchWords(query: IssueQuery): string {
  const where = query.state === "all" ? "these issues" : `${query.state} issues`;
  const parts = [
    ...(query.text ? [`“${query.text}”`] : []),
    ...(query.labels.length > 0 ? [`${query.text ? "with the" : "the"} ${query.labels.length === 1 ? "label" : "labels"} ${query.labels.join(" and ")}`] : []),
    ...(query.assignee === "me" ? ["assigned to you"] : query.assignee === "none" ? ["with no assignee"] : []),
  ];
  return `Nothing in ${where} matches ${sentence(parts)}.`;
}

function IssuesList({ trackers }: { trackers: TrackerRow[] }) {
  const connected = trackers.filter((row) => row.connected);
  const [query, setQuery] = useState<IssueQuery>(() => {
    const last = remembered();
    return { ...noQuery, tracker: last && trackers.some((row) => row.tracker === last) ? last : null };
  });
  const version = useApiVersion();
  const pushToast = useUi((state) => state.pushToast);
  const phone = useLayoutWidth() === "phone";
  const read = useAsync(`issues:list:${version}:${JSON.stringify(query)}`, (signal) => issuesApi().list(query, signal));
  const loading = read.status === "loading";
  const waited = useWaited(loading);
  const issues = useMemo(() => read.data?.issues ?? [], [read.data]);
  const counts = read.data?.counts ?? null;
  // The labels offered are those of the last list that answered, so the panel stays usable while a filter reads again.
  const seen = useRef<{ name: string; count: number }[]>([]);
  if (read.data) seen.current = labelCounts(read.data.issues);
  const [loadedAt, setLoadedAt] = useState<number | null>(null);
  useEffect(() => { if (read.status === "ready" && !read.refreshing) setLoadedAt(Date.now()); }, [read.status, read.refreshing, read.data]);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 30_000); return () => clearInterval(timer); }, []);

  const picked = query.tracker === null ? null : trackers.find((row) => row.tracker === query.tracker) ?? null;
  const change = (next: IssueQuery) => { if (next.tracker !== query.tracker) remember(next.tracker); setQuery(next); };
  const filtered = query.text !== "" || query.labels.length > 0 || query.assignee !== "any";
  const who = query.tracker !== null ? trackerName(query.tracker) : connected.length === 1 ? trackerName(connected[0]!.tracker) : "the tracker";
  const where = picked?.connected && picked.scope ? `${picked.scope} on ${trackerName(picked.tracker)}` : connected.length === 1 ? `${connected[0]!.scope ?? ""} on ${trackerName(connected[0]!.tracker)}` : "your trackers";
  const failure = read.status === "error" && !read.data;

  // The "/" key goes to the search box, unless a field already has the keys.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(input|textarea|select)$/i.test(target.tagName))) return;
      event.preventDefault();
      document.querySelector<HTMLInputElement>(".iss-search input")?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const refresh = (
    <IconButton label="Refresh" aria-busy={loading || read.refreshing || undefined} onClick={() => void read.reload()}>
      {loading || read.refreshing ? <i className="iss-spin spin" aria-hidden /> : <RefreshCw size={14} aria-hidden />}
    </IconButton>
  );
  const status = loading ? "Loading" : read.refreshing ? "Refreshing" : loadedAt ? `Updated ${relativeTime(new Date(loadedAt).toISOString(), now).toLowerCase()}` : "";

  return (
    <>
      <PageBar actions={phone ? refresh : <><span className="iss-upd" role="status">{status}</span>{refresh}</>}>
        <h1 tabIndex={-1} data-page-title>Issues</h1>
      </PageBar>
      <IssuesToolbar
        query={query}
        onQuery={change}
        trackers={trackers}
        counts={counts}
        labels={seen.current}
        shown={read.status === "ready" ? issues.length : null}
        phone={phone}
        loading={loading}
      />
      <div className="iss-table" aria-busy={loading || undefined}>
        {picked && !picked.connected ? (
          <NotConnected rows={trackers} />
        ) : loading ? (
          <>
            <IssuesHead />
            {waited ? <IssuesSkeleton /> : null}
          </>
        ) : failure ? (
          <Unreachable
            who={who}
            onRetry={() => void read.reload()}
            onCopy={() => {
              void navigator.clipboard?.writeText(read.error?.message ?? "").then(() => pushToast("Copied"), () => pushToast("Couldn't copy"));
            }}
          />
        ) : issues.length === 0 && filtered ? (
          <NoMatch words={noMatchWords(query)} onClear={() => change({ ...noQuery, tracker: query.tracker, state: query.state })} />
        ) : issues.length === 0 ? (
          <NoIssues where={where} state={query.state} />
        ) : (
          <>
            <IssuesHead />
            <IssueRows issues={issues} onOpen={(issue: IssueSummary) => navigate({ name: "issues", tracker: issue.tracker, key: issue.key })} />
          </>
        )}
      </div>
    </>
  );
}

export function IssuesPage() {
  const route = useRoute();
  const { rows, settled } = useTrackers();
  const detail = route.name === "issues" && route.tracker !== undefined && route.key !== undefined ? { tracker: route.tracker, key: route.key } : null;
  const body = !settled ? null : rows === null ? (
    <>
      <PageBar><h1 tabIndex={-1} data-page-title>Issues</h1></PageBar>
      <NotReported />
    </>
  ) : !rows.some((row) => row.connected) ? (
    <>
      <PageBar><h1 tabIndex={-1} data-page-title>Issues</h1></PageBar>
      <NotConnected rows={rows} />
    </>
  ) : detail ? (
    <IssueDetail key={`${detail.tracker}:${detail.key}`} tracker={detail.tracker} issueKey={detail.key} />
  ) : (
    <IssuesList trackers={rows} />
  );
  return <div className="iss-page">{body}</div>;
}
