import { Ellipsis, EllipsisVertical, ExternalLink, GitMerge, MessageSquare, Pencil, X } from "lucide-react";
import "../pulls.css";
import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { tabKeys } from "../../../ui/tabKeys";
import { usePageLabel } from "./pageLabel";
import type { PullRequestDetail } from "../../../api/types";
import { agoWords } from "../../../lib/format";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button, IconButton, Pill } from "../../../ui/controls";
import { ConfirmDialog } from "../../../ui/Dialog";
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { PageBar } from "../../../product/ui/PageBar";
import { PullCode } from "./PullCode";
import { PullConversation } from "./PullConversation";
import { PullGuide } from "./PullGuide";
import { pullsApi } from "../api";
import { checkConnectionNow, useConnectionKind } from "../../../product/state/connection";
import { useThreadPulls } from "../model/pullStore";
import { forgeName, numberLabel } from "../../../product/pulls/labels";
import { useVisibleProjects } from "../../../product/state/workspace";
import { PullsBanner } from "../list/PullStates";
import { RequestChangesCard, RequestedChangesEntry } from "./RequestChanges";
import { MergeDialog, deleteBranchChoice, rememberDeleteBranch } from "./MergeDialog";
import { ChecksCard, FilesCard, MergeRow, OfflineLine, PullStateBanner, Row } from "./PullDetailBlocks";
import { baseChangeAllowed, bodyOpensWith, mergeGate, pageState, reviewAskedOf, reviewWords } from "../model/pullModel";
import { Checks, Description, People } from "./PullSummaryCards";
import { Timeline, TimelineList, type TimelineRead, useTimelineEvents } from "./PullTimeline";
import { Stack } from "./PullStack";
import { PAGE_LABELS, pullDetailError, pullDetailLoading, TAB_LABELS, TitleForm, pullMenuItems, type Tab } from "./PullDetailParts";


/** One pull request: Summary, Guide, Timeline and Code, with merge and review. */
export function PullDetail({
  projectId,
  number,
  layout = "panel",
  onClose,
  onOpen,
}: {
  projectId: string;
  number: number;
  /** A page of its own, or the compact view inside a thread's side panel. */
  layout?: "page" | "panel";
  /** In the panel: back to the thread's linked pull requests. */
  onClose?(): void;
  /** Moves to another pull request in the same stack. */
  onOpen?(number: number): void;
}) {
  const detail = useAsync(`pull:${projectId}:${number}`, (signal) => pullsApi.detail(projectId, number, signal), { keep: true });
  const page = layout === "page";
  // What the page layout adds beside the detail: files, checks, the details the list also reads, and who is signed in.
  const fileList = useAsync(`pull-files:${projectId}:${number}:${page}`, (signal) => (page ? pullsApi.files(projectId, number, signal) : Promise.resolve([])));
  const checkList = useAsync(`pull-checklist:${projectId}:${number}:${page}`, (signal) => (page ? pullsApi.checks(projectId, number, signal) : Promise.resolve([])));
  const extras = useAsync(`pull-extras:${projectId}:${number}:${page}`, (signal) =>
    page ? pullsApi.details({ pulls: [{ project_id: projectId, number, updated_at: null }], fresh: false }, signal) : Promise.resolve({ details: [], paused: [] }),
  );
  const forges = useAsync("pulls:forges", (signal) => pullsApi.forges(signal));
  const connection = useConnectionKind();
  const projects = useVisibleProjects();
  const pullUrl = detail.data?.pull.url;
  // The address says which project's pull request, which a number alone does not.
  const linkedThread = useThreadPulls((store) => (pullUrl ? Object.entries(store.byThread).find(([, entry]) => entry.pulls?.some((one) => one.url === pullUrl))?.[0] : undefined));
  const threadTitle = useWorkspace((state) => (linkedThread ? state.threads[linkedThread]?.title : undefined));
  /** The review card is open, and what was sent once it was. */
  const [reviewing, setReviewing] = useState(false);
  const [requested, setRequested] = useState<string | null>(null);
  /** What was typed in the review card, kept for the visit if the card is closed and opened again. */
  const [reviewNote, setReviewNote] = useState("");
  const [confirm, setConfirm] = useState<"close" | "delete_branch" | null>(null);
  const [auto, setAuto] = useState(false);
  const [deleteAfter, setDeleteAfter] = useState(() => deleteBranchChoice(projectId));
  const [mergeMenu, setMergeMenu] = useState<{ x: number; y: number } | null>(null);
  const loadedAt = useMemo(() => (detail.data ? Date.now() : null), [detail.data]);
  const [tab, setTab] = useState<Tab>("summary");
  const [merging, setMerging] = useState(false);
  // The dialog is built from this detail, so it opens on a read that has finished: a kept
  // copy drawn while the read runs could be of a pull request that has moved on.
  const openMerge = () => {
    if (!detail.refreshing) setMerging(true);
    else void detail.reload().then(() => setMerging(true));
  };
  const [mergeMethod, setMergeMethod] = useState("squash");
  const [editing, setEditing] = useState<string | null>(null);
  const [changingBase, setChangingBase] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const pushToast = useUi((state) => state.pushToast);
  const refreshAll = useWorkspace((state) => state.refreshAll);
  const engines = useWorkspace((state) => state.engines.data);
  const tabsId = useId();
  // Once the Conversation tab has been opened the activity stays loaded, so its count does not fall back.
  const [activityWanted, setActivityWanted] = useState(false);
  useEffect(() => {
    if (tab === "timeline") setActivityWanted(true);
  }, [tab]);
  const activity = useTimelineEvents(projectId, number, activityWanted);
  const setPageLabel = usePageLabel((state) => state.set);
  const shownTitle = detail.status === "ready" ? detail.data.pull.title : null;
  useEffect(() => {
    if (layout !== "page" || shownTitle === null) return;
    setPageLabel(shownTitle);
    return () => setPageLabel(null);
  }, [layout, shownTitle, setPageLabel]);

  const forge = forgeName(detail.data?.pull.url);
  const projectName = projects?.find((project) => project.id === projectId)?.name ?? "";
  const signedOut = forges.status === "ready" && forges.data.some((one) => one.installed) && !forges.data.some((one) => one.signed_in);
  const forgeLabel = forge === "The host" ? (forges.data?.find((one) => one.signed_in || one.installed)?.name ?? "GitHub") : forge;
  const pageHead = (title: ReactNode, pill?: ReactNode) => (
    <PageBar back={{ name: "pulls" }}>
      {title}
      <span className="pull-sub">
        #{number} {"\u00b7"} {projectName}
        {pill}
      </span>
    </PageBar>
  );
  if (detail.status === "loading") return pullDetailLoading({ page, number, pageHead });
  if (detail.status === "error" && !detail.data) return pullDetailError({ page, number, pageHead, signedOut, forgeLabel, failure: detail.error, onRetry: detail.reload });
  const current = detail.data as PullRequestDetail;
  const { pull, mergeable, additions, deletions, changed_files: files } = current;
  const open = pull.state === "open";
  const run = async (work: Promise<unknown>, done: string) => {
    try {
      await work;
      pushToast(done);
      void detail.reload();
      // A merge can delete the branch a checkout was on.
      void useWorkspace.getState().refreshManaged();
      // What the banner and the tab counts read goes stale with the pull request.
      if (page) {
        void extras.reload();
        void checkList.reload();
      }
    } catch (failure) {
      pushToast(`Couldn't do that. ${toFailure(failure).message}`);
    }
  };
  const update = (change: Parameters<typeof pullsApi.update>[2], done: string) => run(pullsApi.update(projectId, number, change), done);
  const act = (action: string, done: string) => run(pullsApi.action(projectId, number, action), done);
  const engine = engines?.find((entry) => entry.installed)?.id;
  const work = async () => {
    if (!engine) {
      pushToast("Install an engine first.");
      return;
    }
    try {
      const thread = await pullsApi.work(projectId, number, engine);
      await refreshAll();
      navigate({ name: "thread", id: thread.id });
    } catch (failure) {
      pushToast(`Couldn't start a thread. ${toFailure(failure).message}`);
    }
  };
  const checkout = async (worktree: boolean) => {
    try {
      const done = await pullsApi.checkout(projectId, number, worktree);
      pushToast(done.warning ?? `Checked out ${done.branch}`);
    } catch (failure) {
      pushToast(`Couldn't check it out. ${toFailure(failure).message}`);
    }
  };
  const items = pullMenuItems({ page, pull, current, open, forge, connection, setEditing, setConfirm, setChangingBase, work, update, act, checkout });

  const titleForm = <TitleForm editing={editing} setEditing={setEditing} update={update} />;
  // Plain text, as the frames draw it (pull-detail, pr-detail-cards). The field appears only after Change base branch,
  // which is offered only when the service says the base can be changed; it reports no such permission yet.
  const baseBranch =
    changingBase && open && baseChangeAllowed(current) ? (
      <input
        className="pull-base"
        aria-label="Base branch"
        defaultValue={pull.base}
        key={pull.base}
        autoFocus
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") setChangingBase(false);
        }}
        onBlur={(event) => {
          const base = event.target.value.trim();
          setChangingBase(false);
          if (base && base !== pull.base) void update({ base }, "Base branch changed");
        }}
      />
    ) : (
      pull.base
    );
  const moreActions = (
    <IconButton
      label="More actions"
      onClick={(event) => {
        const rect = event.currentTarget.getBoundingClientRect();
        setMenu({ x: rect.left, y: rect.bottom + 4 });
      }}
    >
      {page ? <Ellipsis size={14} aria-hidden /> : <EllipsisVertical size={14} aria-hidden />}
    </IconButton>
  );
  const order = ["summary", "guide", "timeline", "code"] as const;
  const tabs = order.map((entry) => (
    <button
      key={entry}
      type="button"
      role="tab"
      id={`${tabsId}-${entry}`}
      aria-controls={`${tabsId}-panel`}
      aria-checked={page ? undefined : tab === entry}
      aria-selected={tab === entry}
      tabIndex={tab === entry ? 0 : -1}
      onClick={() => setTab(entry)}
    >
      {TAB_LABELS[entry]}
    </button>
  ));
  const reviewers = (
    <People
      title="Reviewers"
      chosen={current.reviewers}
      load={(signal) => pullsApi.reviewerCandidates(projectId, signal)}
      save={(add, remove) => pullsApi.setReviewers(projectId, number, add, remove).then(detail.reload)}
    />
  );
  const labels = (
    <People
      title="Labels"
      chosen={pull.labels}
      load={(signal) => pullsApi.labelCandidates(projectId, signal)}
      save={(add, remove) => pullsApi.setLabels(projectId, number, add, remove).then(detail.reload)}
    />
  );
  const description = <Description detail={current} projectId={projectId} onSave={(body) => update({ body }, "Description saved")} />;
  const other =
    tab === "guide" ? (
      <PullGuide projectId={projectId} number={number} headSha={current.head_sha} />
    ) : tab === "timeline" ? (
      <Timeline projectId={projectId} number={number} />
    ) : (
      <PullCode projectId={projectId} number={number} onReviewed={detail.reload} />
    );
  const dialogs = (
    <>
      {confirm ? (
        <ConfirmDialog
          title={confirm === "close" ? `Close #${number}?` : "Delete the branch?"}
          confirmLabel={confirm === "close" ? "Close pull request" : "Delete branch"}
          tone="danger"
          cancelFocus
          onClose={() => setConfirm(null)}
          onConfirm={() => run(pullsApi.action(projectId, number, confirm), confirm === "close" ? "Closed" : "Branch deleted")}
        >
          <p>{confirm === "close" ? `${pull.title} is closed without merging. It can be reopened.` : `${pull.head} is deleted from the forge.`}</p>
        </ConfirmDialog>
      ) : null}
      {merging ? (
        <MergeDialog
          projectId={projectId}
          initialMethod={mergeMethod}
          onStale={() => {
            void detail.reload();
            void extras.reload();
            void checkList.reload();
            void fileList.reload();
          }}
          fixedMethod={page}
          initialAuto={page ? auto : undefined}
          initialDeleteBranch={page ? deleteAfter : undefined}
          detail={current}
          onClose={() => setMerging(false)}
          onMerged={() => {
            pushToast("Merged");
            void detail.reload();
            if (page) {
              void extras.reload();
              void checkList.reload();
            }
          }}
        />
      ) : null}
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label="Pull request actions" items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </>
  );

  if (!page) {
    return (
      <article className="pull-detail" aria-label={pull.title}>
        <header className="pull-detail-head">
          <div>
            {editing !== null ? (
              titleForm
            ) : (
              <h2>
                {pull.title}{" "}
                <IconButton label="Edit title" onClick={() => setEditing(pull.title)}>
                  <Pencil size={12} aria-hidden />
                </IconButton>
              </h2>
            )}
            <p>
              <Pill tone={open ? "info" : pull.state === "merged" ? "accent" : "neutral"}>
                {pull.draft ? "Draft" : pull.state}
              </Pill>{" "}
              #{pull.number} · {pull.head} into {baseBranch}
              {additions !== null ? ` · +${additions} -${deletions ?? 0}, ${files ?? 0} files` : ""}
              {current.auto_merge ? " · Merges when checks pass" : ""}
            </p>
          </div>
          <span className="pull-detail-actions">
            {moreActions}
            <a className="icon-btn" href={pull.url} target="_blank" rel="noreferrer" aria-label="Open on the forge">
              <ExternalLink size={14} aria-hidden />
            </a>
            {onClose ? (
              <IconButton label="Close pull request" onClick={onClose}>
                <X size={14} aria-hidden />
              </IconButton>
            ) : null}
          </span>
        </header>
        <Stack projectId={projectId} number={number} onOpen={onOpen} />
        <div className="pull-tabs segment" role="tablist" aria-label="Pull request" onKeyDown={(event) => tabKeys(event, (at) => setTab(order[at] ?? "summary"))}>
          {tabs}
        </div>
        <div className="pull-body" role="tabpanel" id={`${tabsId}-panel`} aria-labelledby={`${tabsId}-${tab}`}>
          {tab === "summary" ? (
            <>
              {description}
              {reviewers}
              {labels}
              <Checks projectId={projectId} number={number} />
              <PullConversation projectId={projectId} number={number} />
            </>
          ) : (
            other
          )}
        </div>
        <footer className="pull-foot">
          <Button small icon={<MessageSquare size={14} aria-hidden />} onClick={() => void work()}>
            Work on this
          </Button>
          {open ? (
            <>
              <Button small onClick={() => setConfirm("close")}>
                Close
              </Button>
              <Button
                small
                tone="primary"
                icon={<GitMerge size={14} aria-hidden />}
                disabled={mergeable === "conflicting"}
                onClick={openMerge}
              >
                Merge
              </Button>
            </>
          ) : pull.state === "closed" ? (
            <Button small onClick={() => void act("reopen", "Reopened")}>
              Reopen
            </Button>
          ) : null}
        </footer>
        {dialogs}
      </article>
    );
  }

  const offline = connection === "offline";
  const viewers = (forges.data ?? []).flatMap((one) => (one.signed_in && one.viewer ? [one.viewer] : []));
  const mine = extras.data?.details[0];
  const reviewAsked = reviewAskedOf(mine, viewers);
  const own = Boolean(pull.author && viewers.some((login) => login.toLowerCase() === (pull.author ?? "").toLowerCase()));
  const state = pageState(pull, mergeable, reviewAsked);
  const failedCheck = (checkList.data ?? []).find((check) => check.state === "fail");
  const gate = mergeGate(pull, mergeable, { offline, auto, forge: forgeLabel });
  const submitReview = (event: "approve" | "request_changes") =>
    run(pullsApi.review(projectId, number, event, ""), event === "approve" ? "Approved" : "Changes requested");
  const forgeLink = (
    <a className="btn btn-secondary btn-small pull-forge" href={pull.url} target="_blank" rel="noreferrer" aria-disabled={offline || undefined} tabIndex={offline ? -1 : undefined} onClick={(event) => offline && event.preventDefault()}>
      Open on {forge === "The host" ? "the forge" : forge}
    </a>
  );
  const pageOrder = ["summary", "guide", "timeline", "code", "checks"] as const;
  const counts: Partial<Record<Tab, number | null | undefined>> = {
    // What the tab lists: comments, reviews and commits alike. Until it has loaded, the comments alone.
    timeline: activity.data ? activity.data.length : mine?.comment_count,
    code: files === null ? undefined : Number(files),
    checks: pull.checks?.total,
  };
  const pageTabs = pageOrder.map((entry) => (
    <button
      key={entry}
      type="button"
      role="tab"
      id={`${tabsId}-${entry}`}
      aria-controls={`${tabsId}-panel`}
      aria-selected={tab === entry}
      tabIndex={tab === entry ? 0 : -1}
      onClick={() => setTab(entry)}
    >
      {PAGE_LABELS[entry]}
      {counts[entry] !== undefined && counts[entry] !== null ? ` \u00b7 ${counts[entry]}` : ""}
    </button>
  ));
  const pillTone = open && !pull.draft ? "info" : pull.state === "merged" ? "accent" : "neutral";
  const statusWord = pull.draft ? "Draft" : pull.state[0]!.toUpperCase() + pull.state.slice(1);
  const failing = pull.checks?.failing ?? 0;

  return (
    <article className="pull-page" aria-label={pull.title} data-state={state}>
      <PageBar
        back={{ name: "pulls" }}
        actions={
          <>
            {forgeLink}
            {moreActions}
          </>
        }
      >
        {editing !== null ? (
          titleForm
        ) : (
          <h1 tabIndex={-1} data-page-title>
            {pull.title}
          </h1>
        )}
        <span className="pull-sub">
          {numberLabel(pull)} {"\u00b7"} {projectName}
          <Pill tone={pillTone}>{statusWord}</Pill>
          {requested !== null ? <span className="pull-changes-label">Changes requested</span> : null}
        </span>
      </PageBar>
      {offline ? (
        <OfflineLine
          ago={agoWords(loadedAt ?? Date.now(), Date.now())}
          onRetry={() => {
            checkConnectionNow();
            detail.reload();
          }}
        />
      ) : tab === "checks" && failing > 0 ? (
        <PullsBanner tone="err" icon={<X size={14} aria-hidden />} actions={null}>
          {failing} {failing === 1 ? "check" : "checks"} failed
        </PullsBanner>
      ) : requested !== null ? null : (
        <PullStateBanner
          pull={pull}
          state={state}
          failed={failedCheck}
          own={own}
          reviewing={reviewing}
          actions={{
            reopen: () => void act("reopen", "Reopened"),
            ready: () => void update({ draft: false }, "Ready for review"),
            approve: () => void submitReview("approve"),
            requestChanges: () => setReviewing(true),
            deleteBranch: () => setConfirm("delete_branch"),
          }}
        />
      )}
      <Stack projectId={projectId} number={number} onOpen={onOpen} />
      <div className="pull-page-tabs" role="tablist" aria-label="Pull request" onKeyDown={(event) => tabKeys(event, (at) => setTab(pageOrder[at] ?? "summary"))}>
        {pageTabs}
      </div>
      <div className="pull-page-body" role="tabpanel" id={`${tabsId}-panel`} aria-labelledby={`${tabsId}-${tab}`}>
        {tab === "guide" || tab === "code" || tab === "timeline" || tab === "checks" ? (
          <div className="pull-cols">
            <div className="pull-main">
              {tab === "guide" ? (
                <PullGuide projectId={projectId} number={number} headSha={current.head_sha} />
              ) : tab === "code" ? (
                <PullCode projectId={projectId} number={number} shared={fileList} onReviewed={detail.reload} />
              ) : tab === "timeline" ? (
                <>
                  <PullConversation projectId={projectId} number={number} />
                  <TimelineList events={activity as TimelineRead} />
                </>
              ) : (
                <Checks projectId={projectId} number={number} />
              )}
            </div>
            <div className="pull-side">{sideColumn()}</div>
          </div>
        ) : (
          <div className="pull-cols">
            <div className="pull-main">
              <div className="pull-card pull-summary">
                {bodyOpensWith(current.body, "Summary") ? null : <b>Summary</b>}
                {description}
              </div>
              {requested !== null ? <RequestedChangesEntry note={requested} /> : null}
              <FilesCard files={fileList.data ?? []} additions={additions === null ? null : Number(additions)} deletions={deletions === null ? null : Number(deletions)} />
            </div>
            <div className="pull-side">{sideColumn()}</div>
          </div>
        )}
      </div>
      {dialogs}
    </article>
  );

  /** Status, checks and merge: the same beside every tab. */
  function sideColumn() {
    return (
      <>
        <div className="pull-card pull-status">
          <Row name="Status">{statusWord}</Row>
          <Row name="Branch">
            <span className="pull-mono">
              {pull.head} {"\u2192"} {baseBranch}
            </span>
          </Row>
          <Row name="Review" tone={state === "review" || state === "changes" ? "warn" : undefined}>
            {reviewWords(pull, current.reviewers)}
          </Row>
          <Row name="Conflicts" tone={mergeable === "conflicting" ? "warn" : undefined}>
            {mergeable === "conflicting" ? "Has conflicts" : mergeable === "mergeable" ? "None" : "\u2014"}
          </Row>
          {threadTitle ? (
            <Row name="Thread">
              <a href={`#/threads/${linkedThread}`}>{threadTitle}</a>
            </Row>
          ) : null}
        </div>
        <ChecksCard pull={pull} checks={checkList.data} collapsed={reviewing} />
        <MergeRow
          method={mergeMethod}
          onMethod={setMergeMethod}
          auto={auto}
          onAuto={setAuto}
          deleteBranch={deleteAfter}
          onDeleteBranch={(on) => {
            setDeleteAfter(on);
            rememberDeleteBranch(projectId, on);
          }}
          gate={current.auto_merge ? { ...gate, hint: "Merges when checks pass" } : requested !== null ? { ...gate, hint: "You requested changes." } : gate}
          menu={mergeMenu}
          onMenu={setMergeMenu}
          onMerge={openMerge}
          hidden={pull.state === "merged"}
          dim={reviewing}
        />
        {reviewing ? (
          <RequestChangesCard
            projectId={projectId}
            number={number}
            own={own}
            ownKnown={forges.status === "ready"}
            note={reviewNote}
            onNote={setReviewNote}
            forge={forgeLabel}
            onCancel={() => setReviewing(false)}
            onDone={(note) => {
              setReviewing(false);
              setReviewNote("");
              setRequested(note);
              void detail.reload();
              void extras.reload();
            }}
            onUnknown={() => {
              void detail.reload();
              void extras.reload();
            }}
          />
        ) : null}
        {reviewers}
        {labels}
      </>
    );
  }
}
