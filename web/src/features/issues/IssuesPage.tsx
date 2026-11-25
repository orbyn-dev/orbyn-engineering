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
import { issuesApi, markNotOffered, useApiVersion, useTrackers } from "./api";
import type { IssueList, IssueQuery, IssueSummary, TrackerKind, TrackerRow } from "./contract";
import { NewIssue } from "./IssueCreate";
import { IssueDetail } from "./IssueDetail";
import { IssueRows, IssuesHead, IssuesSkeleton, LoadMoreRow } from "./IssueRows";
import { NoIssues, NoMatch, NotConnected, NotReported, Unreachable } from "./IssueStates";
import { IssuesToolbar } from "./IssuesToolbar";
import { isTracker, labelCounts, noQuery, reasonOf, trackerName } from "./model";

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
  // A sorted list that the service turns down (a 400, or "no such route") means it does not take `sort`: the control goes back to
  // Planned and the list is asked again in the service's own order. No sorted list is ever shown that was not sorted.
  useEffect(() => {
    if (read.status === "error" && query.sort !== undefined && [400, 404, 405].includes(read.error.status)) {
      markNotOffered("sort");
      setQuery((now) => ({ ...now, sort: undefined }));
    }
  }, [read.status, read.error, query.sort]);
  const loading = read.status === "loading";
  const waited = useWaited(loading);
  // The pages after the first, kept with the answer they follow: a new first answer (a filter, a refresh) drops them.
  const [later, setLater] = useState<{ of: IssueList; issues: IssueSummary[]; next: Record<string, string> } | null>(null);
  const [paging, setPaging] = useState(false);
  // What the last page said when it failed, kept next to Load more; a new first answer, a retry or a page that lands drops it.
  const [pageFailure, setPageFailure] = useState<string | null>(null);
  const pagingNow = useRef(false);
  const pageRequest = useRef<AbortController | null>(null);
  const more = later && later.of === read.data ? later : null;
  const issues = useMemo(() => {
    const first = read.data?.issues ?? [];
    return more ? [...first, ...more.issues] : first;
  }, [read.data, more]);
  const nextCursors = more ? more.next : read.data?.next_cursors ?? {};
  // A page still on its way when the first answer changes is dropped: the latest request wins.
  useEffect(() => {
    pageRequest.current?.abort();
    pageRequest.current = null;
    pagingNow.current = false;
    setPaging(false);
    setPageFailure(null);
  }, [read.data]);
  useEffect(() => () => pageRequest.current?.abort(), []);
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

  const loadMore = async () => {
    const first = read.data;
    if (pagingNow.current || !first || Object.keys(nextCursors).length === 0) return;
    pagingNow.current = true;
    setPaging(true);
    setPageFailure(null);
    pageRequest.current?.abort();
    const request = new AbortController();
    pageRequest.current = request;
    try {
      // The cursors go back exactly as the service gave them, with the same filters.
      const page = await issuesApi().list(query, request.signal, nextCursors);
      if (request.signal.aborted) return;
      const have = new Set([...first.issues, ...(more?.issues ?? [])].map((issue) => `${issue.tracker}:${issue.scope}:${issue.key}`));
      const fresh = page.issues.filter((issue) => !have.has(`${issue.tracker}:${issue.scope}:${issue.key}`));
      setLater({ of: first, issues: [...(more?.issues ?? []), ...fresh], next: page.next_cursors });
    } catch (failure) {
      if (!request.signal.aborted) {
        pushToast(`Couldn't load more. ${reasonOf(failure)}`);
        setPageFailure(`Couldn’t load more. ${reasonOf(failure)}`);
      }
    } finally {
      if (pageRequest.current === request) {
        pageRequest.current = null;
        pagingNow.current = false;
        setPaging(false);
      }
    }
  };

  const refresh = (
    <IconButton label="Refresh" aria-busy={loading || read.refreshing || undefined} onClick={() => void read.reload()}>
      {loading || read.refreshing ? <i className="iss-spin spin" aria-hidden /> : <RefreshCw size={14} aria-hidden />}
    </IconButton>
  );
  // The page bar's one action (issue-write-new): it reads the trackers the list already has, and Check the list re-reads this list.
  const newIssue = <NewIssue trackers={trackers} preferred={query.tracker} onCheckList={() => void read.reload()} />;
  const status = loading ? "Loading" : read.refreshing ? "Refreshing" : loadedAt ? `Updated ${relativeTime(new Date(loadedAt).toISOString(), now).toLowerCase()}` : "";

  return (
    <>
      <PageBar actions={phone ? refresh : <>{newIssue}<span className="iss-upd" role="status">{status}</span>{refresh}</>}>
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
          <NoMatch words={noMatchWords(query)} onClear={() => change({ ...noQuery, tracker: query.tracker, state: query.state, sort: query.sort })} />
        ) : issues.length === 0 ? (
          <NoIssues where={where} state={query.state} />
        ) : (
          <>
            <IssuesHead />
            <IssueRows issues={issues} onOpen={(issue: IssueSummary) => navigate({ name: "issues", tracker: issue.tracker, scope: issue.scope, key: issue.key })} />
            {Object.keys(nextCursors).length > 0 ? <LoadMoreRow busy={paging} failure={pageFailure} onLoad={() => void loadMore()} /> : null}
          </>
        )}
      </div>
    </>
  );
}

export function IssuesPage() {
  const route = useRoute();
  const { rows, settled } = useTrackers();
  const detail = route.name === "issues" && route.tracker !== undefined && route.scope !== undefined && route.key !== undefined ? { tracker: route.tracker, scope: route.scope, key: route.key } : null;
  // The Facts show the host the service reports for this issue's tracker, from the rows this page already reads.
  const host = detail && rows ? rows.find((row) => row.tracker === detail.tracker)?.host ?? null : null;
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
    <IssueDetail key={`${detail.tracker}:${detail.scope}:${detail.key}`} tracker={detail.tracker} scope={detail.scope} issueKey={detail.key} host={host} />
  ) : (
    <IssuesList trackers={rows} />
  );
  return <div className="iss-page">{body}</div>;
}
