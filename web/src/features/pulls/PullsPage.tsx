import { RefreshCw, Search, X } from "lucide-react";
import "./pulls.css";
import { useEffect, useMemo, useRef, useState } from "react";
import type { PullList, PullListEntry } from "../../api/types";
import { relativeTime } from "../../lib/format";
import { useWaited } from "../../lib/motion";
import { deriveProjectIdentity } from "../../lib/projectIdentity";
import { ApiFailure } from "../../api/client";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { navigate, useRoute } from "../../state/route";
import { useUi } from "../../state/ui";
import { useVisibleProjects } from "../../state/workspace";
import { Button, IconButton } from "../../ui/controls";
import { checkConnectionNow, useConnectionKind } from "../../state/connection";
import { agoWords } from "../thread/pullStore";
import { ConfirmDialog } from "../../ui/Dialog";
import { PageBar } from "../../ui/PageBar";
import { ErrorState } from "../../ui/states";
import { PullDetail } from "./PullDetail";
import { RowMergeDialog } from "./MergeDialog";
import { PullsToolbar } from "./PullsToolbar";
import { PullRows, PullsHead, PullsSkeleton } from "./PullRows";
import { DetailsBanner, NoPulls, OfflineBanner, RateLimitBanner, SignedOut, Unreachable } from "./PullStates";
import { usePullDetails } from "./usePullDetails";
import {
  activeChips,
  clearedFilters,
  defaultSort,
  labelsOf,
  detailsKey,
  needsDetails,
  noFilters,
  statesToAsk,
  visiblePulls,
  withoutDetailFilters,
  authorsOf,
  type PullData,
  type PullFilters,
  type PullSort,
} from "./pullModel";
import { pullsApi } from "../../api/pulls";
/** One request per state the service is asked for, merged into one list. */
async function loadPulls(states: readonly string[], query: string, signal: AbortSignal): Promise<PullList> {
  const lists = await Promise.all(states.map((state) => pullsApi.list(state, query, signal)));
  const seen = new Set<string>();
  const pulls = lists.flatMap((list) => list.pulls).filter((entry) => {
    const key = `${entry.project_id}:${entry.pull.number}`;
    return seen.has(key) ? false : (seen.add(key), true);
  });
  const failed = lists.flatMap((list) => list.failed).filter((one, at, all) => all.findIndex((other) => other.project_id === one.project_id) === at);
  return { pulls, failed };
}

const sentence = (parts: string[]) => (parts.length < 2 ? (parts[0] ?? "") : `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}`);

export function PullsPage() {
  const [text, setText] = useState("");
  const [tokens, setTokens] = useState<string[]>([]);
  const [submitted, setSubmitted] = useState("");
  const [sort, setSort] = useState<PullSort>(defaultSort);
  const [filters, setFilters] = useState<PullFilters>(noFilters);
  const route = useRoute();
  const selected = route.name === "pulls" && route.project !== undefined && route.number !== undefined ? { project: route.project, number: route.number } : null;
  const setSelected = (next: { project: string; number: number } | null) =>
    navigate(next ? { name: "pulls", project: next.project, number: next.number } : { name: "pulls" });
  const [picked, setPicked] = useState<PullListEntry[]>([]);
  const [merging, setMerging] = useState<PullListEntry | null>(null);
  const [closing, setClosing] = useState<PullListEntry | null>(null);
  const pushToast = useUi((state) => state.pushToast);
  const projects = useVisibleProjects();
  const phone = useLayoutWidth() === "phone";
  const connection = useConnectionKind();
  const asked = statesToAsk(filters.states);
  const list = useAsync(`pulls:${[...asked].sort().join("+")}:${submitted}`, (signal) => loadPulls(asked, submitted, signal));
  const forges = useAsync("pulls:forges", (signal) => pullsApi.forges(signal));
  const all = useMemo(() => list.data?.pulls ?? [], [list.data]);
  const details = usePullDetails(all);
  const signedIn = (forges.data ?? []).filter((forge) => forge.signed_in);
  const viewers = useMemo(() => (forges.data ?? []).flatMap((forge) => (forge.signed_in && forge.viewer ? [forge.viewer] : [])), [forges.data]);
  const forgeLabel = signedIn[0]?.name ?? (forges.data ?? []).find((forge) => forge.installed)?.name ?? "GitHub";
  const signedOut = forges.status === "ready" && forges.data.some((forge) => forge.installed) && signedIn.length === 0;
  // What the details have brought: each control that needs one appears once it exists.
  const has = useMemo<PullData>(() => {
    const known = [...details.byKey.values()];
    return {
      viewerLogin: viewers.length > 0,
      reviewRequests: viewers.length > 0 && known.some((one) => one.requested_reviewers !== undefined),
      commentCount: known.some((one) => one.comment_count !== undefined),
      createdAt: known.some((one) => one.created_at !== undefined),
    };
  }, [details.byKey, viewers]);
  // A filter never runs on missing data: while details are on their way the unfiltered rows stay,
  // and after a failure they stay with a banner.
  const missing = all.some(({ project_id, pull }) => !details.byKey.has(detailsKey(project_id, pull.number)));
  // The rate limit pausing reviewers is the same as not having them: the filter cannot apply yet.
  // A check at the reset time keeps the pause on screen until its answer says otherwise.
  const rateLimited = details.paused.includes("requested_reviewers");
  const blocked = needsDetails(filters) && ((missing && details.status !== "ready") || rateLimited);
  const detailsFailed = blocked && details.status === "failed" && !rateLimited;
  const paused = blocked && rateLimited;
  const filterWaiting = blocked && !detailsFailed && !paused;
  // Without a creation time the number orders by date, which only means something inside one repository.
  const oneRepo = has.createdAt || filters.project !== null || new Set(all.map((entry) => entry.project_id)).size <= 1;
  const orderBy = oneRepo || sort.by !== "created" ? sort : defaultSort;
  const rows = useMemo(
    () => visiblePulls(all, blocked ? withoutDetailFilters(filters) : filters, tokens, orderBy, viewers, details.byKey),
    [all, blocked, filters, tokens, orderBy, viewers, details.byKey],
  );
  const refresh = () => void list.reload();
  // Coming back from a pull request, by any way, reads the list again: what it did may have changed a row.
  const wasOpen = useRef(false);
  const open = selected !== null;
  useEffect(() => {
    if (wasOpen.current && !open) refresh();
    wasOpen.current = open;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  /** The row may leave the list: focus goes to the row now in its place, or else the heading. */
  const afterRowAction = (entry: PullListEntry) => {
    const at = rows.findIndex((one) => one.project_id === entry.project_id && one.pull.number === entry.pull.number);
    void list.reload().then(() =>
      requestAnimationFrame(() => {
        const buttons = document.querySelectorAll<HTMLElement>(".pull-rows .pull-open");
        (buttons[Math.min(Math.max(at, 0), buttons.length - 1)] ?? document.querySelector<HTMLElement>("[data-page-title]"))?.focus();
      }),
    );
  };
  const refreshByPerson = () => {
    details.markFresh();
    refresh();
  };
  const loading = list.status === "loading";
  const refreshing = list.status === "ready" && list.refreshing;
  // A load that finishes inside 400 ms shows nothing under the header. A longer one shows the
  // static skeleton, and the rows then fade in as it leaves.
  const skeletonShown = useWaited(loading) || (filterWaiting && details.waited);
  const [revealed, setRevealed] = useState(false);
  useEffect(() => {
    if (loading) setRevealed(skeletonShown);
  }, [loading, skeletonShown]);
  const loadedAt = useMemo(() => (list.data ? Date.now() : null), [list.data]);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  const quick = (work: Promise<unknown>, done: string) => {
    work.then(
      () => {
        pushToast(done);
        refresh();
      },
      (failure: unknown) => pushToast(`Couldn't do that. ${toFailure(failure).message}`),
    );
  };
  const closePicked = async () => {
    const byProject = new Map<string, number[]>();
    for (const { project_id: project, pull } of picked) byProject.set(project, [...(byProject.get(project) ?? []), pull.number]);
    try {
      const results = await Promise.all([...byProject].map(([project, numbers]) => pullsApi.closeMany(project, numbers)));
      const failed = results.flat().filter((result) => !result.ok);
      pushToast(failed.length > 0 ? `Couldn't close ${failed.map((result) => `#${result.number}`).join(", ")}` : `Closed ${picked.length}`);
      setPicked([]);
      refresh();
    } catch (failure) {
      pushToast(`Couldn't close those. ${toFailure(failure).message}`);
    }
  };
  const nameOf = (id: string) => projects?.find((project) => project.id === id)?.name ?? "";
  const count = (test: (pull: PullListEntry["pull"]) => boolean) => all.filter(({ pull }) => test(pull)).length;
  const toolbarProjects = (projects ?? []).map((project) => ({
    id: project.id,
    name: project.name,
    color: project.project_icon && project.project_icon.kind !== "emoji" ? project.project_icon.color : deriveProjectIdentity(project.name).color,
    count: all.filter((entry) => entry.project_id === project.id).length,
  }));
  const labelCounts = labelsOf(all).map((name) => ({ name, count: count((pull) => pull.labels.includes(name)) }));
  const requesters = useMemo(() => {
    const names = new Map<string, number>();
    for (const one of details.byKey.values()) for (const who of one.requested_reviewers ?? []) if (who.kind === "user") names.set(who.name, (names.get(who.name) ?? 0) + 1);
    return [...names.keys()].filter((name) => !viewers.some((login) => login.toLowerCase() === name.toLowerCase())).sort();
  }, [details.byKey, viewers]);
  const chips = activeChips(filters);
  const criteria = [
    ...(submitted ? [`"${submitted}"`] : []),
    ...tokens.map((token) => `label:${token}`),
    ...chips.map((chip) => chip.label),
    ...(filters.project ? [`Project: ${nameOf(filters.project)}`] : []),
  ];
  const clearAll = () => {
    setFilters(clearedFilters({ ...filters, project: null }));
    setTokens([]);
    setText("");
    setSubmitted("");
  };
  const refreshButton = (
    <IconButton label="Refresh" aria-busy={loading || refreshing || undefined} onClick={refreshByPerson}>
      {loading || refreshing ? <i className="pulls-refresh-spin spin" aria-hidden /> : <RefreshCw size={14} aria-hidden />}
    </IconButton>
  );
  const env = { status: details.status, waited: details.waited, paused: details.paused, viewers };
  const failure = list.status === "error" && !list.data;
  const unreachable = list.status === "ready" && all.length === 0 && list.data.failed.length > 0;

  if (selected) {
    return (
      <div className="pulls-page">
        <PullDetail
          layout="page"
          key={`${selected.project}:${selected.number}`}
          projectId={selected.project}
          number={selected.number}
          onOpen={(number) => setSelected({ project: selected.project, number })}
        />
      </div>
    );
  }

  return (
    <div className="pulls-page">
      <PageBar
        actions={
          phone ? undefined : (
            <>
              {details.status === "failed" ? (
                <span className="pulls-upd pulls-upd-details">
                  Details unavailable {"\u00b7"}{" "}
                  <button type="button" className="pulls-retry" aria-label="Retry loading details" onClick={details.retry}>
                    Retry
                  </button>
                </span>
              ) : null}
              <span className="pulls-upd" role="status">
                {loading ? "Loading" : refreshing ? "Refreshing" : loadedAt ? `Updated ${relativeTime(new Date(loadedAt).toISOString(), now).toLowerCase()}` : ""}
              </span>
              {refreshButton}
            </>
          )
        }
      >
        <h1 tabIndex={-1} data-page-title>
          Pull requests
        </h1>
      </PageBar>
      {signedOut ? (
        <SignedOut
          forge={forgeLabel}
          onSignIn={() => navigate({ name: "settings", section: "source-control" })}
          onLater={() => (window.history.length > 1 ? window.history.back() : navigate({ name: "home" }))}
        />
      ) : (
        <>
          {connection === "offline" && list.data && loadedAt ? (
            <OfflineBanner
              ago={agoWords(loadedAt, now)}
              onRetry={() => {
                checkConnectionNow();
                refresh();
              }}
            />
          ) : null}
          <PullsToolbar
            openCount={count((pull) => pull.state === "open")}
            draftCount={count((pull) => pull.state === "open" && pull.draft)}
            loading={loading}
            unavailable={failure || unreachable}
            filters={filters}
            onFilters={setFilters}
            sort={orderBy}
            oneRepo={oneRepo}
            has={has}
            onSort={setSort}
            text={text}
            onText={setText}
            tokens={tokens}
            onTokens={setTokens}
            onSubmit={setSubmitted}
            projects={toolbarProjects}
            allCount={all.length}
            authors={authorsOf(all)}
            labels={labelCounts}
            requesters={requesters}
            viewer={viewers[0] ?? null}
            shown={rows.length}
            phone={phone}
            phoneRefresh={phone ? refreshButton : undefined}
            detailsWait={filterWaiting ? "waiting" : detailsFailed ? "failed" : paused ? "paused" : undefined}
          />
          {phone && details.status === "failed" ? (
            <p className="pulls-upd pulls-upd-details pulls-upd-phone">
              Details unavailable {"\u00b7"}{" "}
              <button type="button" className="pulls-retry" aria-label="Retry loading details" onClick={details.retry}>
                Retry
              </button>
            </p>
          ) : null}
          {paused ? <RateLimitBanner forge={forgeLabel} resumeAt={details.resumeAt} still={details.stillLimited} nextCheckAt={details.nextCheckAt} /> : null}
          {detailsFailed ? (
            <DetailsBanner
              onRetry={details.retry}
              onRemove={() => setFilters(withoutDetailFilters(filters))}
            />
          ) : null}
          <div className="pulls-table" aria-busy={filterWaiting || undefined}>
            <span className="sr-only" aria-live="polite">
              {filterWaiting ? "Loading details" : ""}
            </span>
            {picked.length > 0 ? (
              <div className="pulls-bulk" role="status">
                <span>{picked.length} selected</span>
                <Button small tone="danger" onClick={() => void closePicked()}>
                  Close selected
                </Button>
                <Button small tone="quiet" onClick={() => setPicked([])}>
                  Clear
                </Button>
              </div>
            ) : null}
            {loading || filterWaiting ? (
              <>
                <PullsHead />
                {skeletonShown ? <PullsSkeleton /> : filterWaiting ? <PullRows {...rowProps()} /> : null}
              </>
            ) : failure ? (
              <ErrorState title="Couldn't load pull requests" failure={list.error ?? new ApiFailure("unexpected", "Something went wrong.", 0)} onRetry={list.reload} />
            ) : unreachable ? (
              <Unreachable
                forge={forgeLabel}
                onRetry={refresh}
                onCopy={() => {
                  const lines = (list.data?.failed ?? []).map((one) => `${nameOf(one.project_id)}: ${one.message}`).join("\n");
                  void navigator.clipboard?.writeText(lines).then(
                    () => pushToast("Copied"),
                    () => pushToast("Couldn't copy"),
                  );
                }}
              />
            ) : rows.length === 0 && criteria.length > 0 ? (
              <div className="pulls-nomatch">
                <Search size={20} aria-hidden />
                <b>No pull requests match</b>
                <p>
                  {filters.project
                    ? `No pull requests in ${nameOf(filters.project)} match these filters.`
                    : `Nothing in ${sentence(filters.states.map((state) => state[0]!.toUpperCase() + state.slice(1)))} matches ${sentence(criteria)}.`}
                </p>
                <Button icon={<X size={14} aria-hidden />} tone="primary" onClick={clearAll}>
                  Clear filters
                </Button>
              </div>
            ) : rows.length === 0 ? (
              <NoPulls />
            ) : (
              <>
                <PullsHead />
                <PullRows {...rowProps()} />
              </>
            )}
          </div>
        </>
      )}
      {merging ? <RowMergeDialog
          projectId={merging.project_id}
          number={merging.pull.number}
          onClose={() => setMerging(null)}
          onStale={refresh}
          onMerged={() => {
            pushToast("Merged");
            afterRowAction(merging);
          }}
        /> : null}
      {closing ? (
        <ConfirmDialog
          title={`Close #${closing.pull.number}?`}
          confirmLabel="Close pull request"
          tone="danger"
          cancelFocus
          onClose={() => setClosing(null)}
          onConfirm={() => pullsApi.action(closing.project_id, closing.pull.number, "close").then(() => (pushToast("Closed"), afterRowAction(closing)))}
        >
          <p>{closing.pull.title} is closed without merging. It can be reopened.</p>
        </ConfirmDialog>
      ) : null}
    </div>
  );

  function rowProps() {
    return {
      rows,
      details: details.byKey,
      env,
      picked,
      revealed,
      online: connection !== "offline",
      projectName: nameOf,
      onPick: (entry: PullListEntry) =>
        setPicked(
          picked.some((one) => one.project_id === entry.project_id && one.pull.number === entry.pull.number)
            ? picked.filter((one) => !(one.project_id === entry.project_id && one.pull.number === entry.pull.number))
            : [...picked, entry],
        ),
      onOpen: (entry: PullListEntry) => setSelected({ project: entry.project_id, number: entry.pull.number }),
      onReopen: (entry: PullListEntry) => quick(pullsApi.action(entry.project_id, entry.pull.number, "reopen"), "Reopened"),
      onReady: (entry: PullListEntry) => quick(pullsApi.update(entry.project_id, entry.pull.number, { draft: false }), "Ready for review"),
      onMerge: (entry: PullListEntry) => setMerging(entry),
      onClose: (entry: PullListEntry) => setClosing(entry),
    };
  }
}
