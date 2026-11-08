import { Check, CircleDashed, Ellipsis, EllipsisVertical, ExternalLink, GitMerge, Layers, MessageSquare, Pencil, Plus, X } from "lucide-react";
import "./pulls.css";
import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { tabKeys } from "../../ui/tabKeys";
import { usePageLabel } from "../../state/pageLabel";
import type { PullCheck, PullRequestDetail } from "../../api/types";
import { relativeTime } from "../../lib/format";
import { toFailure, useAsync } from "../../lib/useAsync";
import { navigate } from "../../state/route";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { Button, IconButton, Pill } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { PageBar } from "../../ui/PageBar";
import { ErrorState, Loading } from "../../ui/states";
import { Markdown, useThreadPulls, agoWords, forgeName, numberLabel } from "../thread/public";
import { PullCode } from "./PullCode";
import { PullConversation } from "./PullConversation";
import { PullGuide } from "./PullGuide";
import { pullsApi } from "../../api/pulls";
import { checkConnectionNow, useConnectionKind } from "../../state/connection";
import { useVisibleProjects } from "../../state/workspace";
import { PullsBanner } from "./PullStates";
import { RequestChangesCard, RequestedChangesEntry } from "./RequestChanges";
import { MergeDialog, deleteBranchChoice, rememberDeleteBranch } from "./MergeDialog";
import { ChecksCard, FilesCard, MergeRow, OfflineLine, PullPageSkeleton, PullStateBanner, Row, SignedOutBlock, UnreachableBlock } from "./PullPage";
import { bodyOpensWith, mergeGate, pageState, reviewAskedOf, reviewWords } from "./pullModel";

type Tab = "summary" | "guide" | "timeline" | "code" | "checks";

const TAB_LABELS: Record<Tab, string> = { summary: "Summary", guide: "Guide", timeline: "Timeline", code: "Code", checks: "Checks" };
const PAGE_LABELS: Record<Tab, string> = { summary: "Summary", guide: "Guide", timeline: "Conversation", code: "Files changed", checks: "Checks" };

const checkIcon = (check: PullCheck) =>
  check.state === "pass" ? (
    <Check size={14} className="tone-success" role="img" aria-label="Passing" />
  ) : check.state === "fail" ? (
    <X size={14} className="tone-danger" role="img" aria-label="Failing" />
  ) : (
    <CircleDashed size={14} className="tone-warn" role="img" aria-label="Pending" />
  );

/** A fork's workflow waits for approval on a run named only by its link. */
const waitingRun = (check: PullCheck): number | null => {
  if (check.state !== "pending" || !/approv/i.test(check.description ?? "")) return null;
  const run = /\/actions\/runs\/(\d+)/.exec(check.url ?? "");
  return run ? Number(run[1]) : null;
};

function Checks({ projectId, number }: { projectId: string; number: number }) {
  const checks = useAsync(`pull-checks:${projectId}:${number}`, (signal) => pullsApi.checks(projectId, number, signal), { keep: true });
  const pushToast = useUi((state) => state.pushToast);
  if (checks.status !== "ready" || checks.data.length === 0) return null;
  const approve = (run: number) => {
    pullsApi.approveWorkflow(projectId, run).then(
      () => {
        pushToast("Workflow approved");
        checks.reload();
      },
      (failure: unknown) => pushToast(`Couldn't approve it. ${toFailure(failure).message}`),
    );
  };
  return (
    <section className="pull-section pull-card">
      <h3>Checks</h3>
      <ul className="pull-checks">
        {checks.data.map((check) => {
          const run = waitingRun(check);
          return (
            <li key={`${check.workflow ?? ""}:${check.name}`}>
              {checkIcon(check)}
              <span>{check.name}</span>
              {check.description ? <small className="muted">{check.description}</small> : null}
              {check.url ? (
                <a href={check.url} target="_blank" rel="noreferrer" aria-label={`Open ${check.name}`}>
                  <ExternalLink size={12} aria-hidden />
                </a>
              ) : null}
              {run !== null ? (
                <Button small onClick={() => approve(run)}>
                  Approve workflow
                </Button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** Reviewers or labels: what's set, and a search over what the forge offers. */
function People({
  title,
  chosen,
  load,
  save,
}: {
  title: string;
  chosen: string[];
  load(signal: AbortSignal): Promise<{ name: string }[]>;
  save(add: string[], remove: string[]): Promise<void>;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const candidates = useAsync(`pull-${title}`, load);
  const pushToast = useUi((state) => state.pushToast);
  const change = (add: string[], remove: string[]) => {
    save(add, remove).catch((failure: unknown) => pushToast(`Couldn't change that. ${toFailure(failure).message}`));
  };
  const items: MenuItem[] = (candidates.data ?? []).map((candidate) => {
    const on = chosen.includes(candidate.name);
    return {
      label: candidate.name,
      selected: on,
      onSelect: () => (on ? change([], [candidate.name]) : change([candidate.name], [])),
    };
  });
  return (
    <div className="pull-people pull-card">
      <span className="pull-card-title">{title}</span>
      {chosen.map((name) => (
        <Pill key={name}>{name}</Pill>
      ))}
      <IconButton
        label={`Change ${title.toLowerCase()}`}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <Plus size={14} aria-hidden />
      </IconButton>
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label={title} items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </div>
  );
}

function Description({
  detail,
  projectId,
  onSave,
}: {
  detail: PullRequestDetail;
  projectId: string;
  onSave(body: string): Promise<void>;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  if (draft !== null) {
    return (
      <div className="pull-section">
        <textarea aria-label="Description" rows={6} value={draft} onChange={(event) => setDraft(event.target.value)} />
        <div className="pull-form-actions">
          <Button small onClick={() => setDraft(null)}>
            Cancel
          </Button>
          <Button small tone="primary" onClick={() => void onSave(draft).then(() => setDraft(null))}>
            Save description
          </Button>
        </div>
      </div>
    );
  }
  return (
    <>
      {detail.body ? <Markdown text={detail.body} projectId={projectId} /> : <p className="muted">No description provided.</p>}
      <IconButton label="Edit description" onClick={() => setDraft(detail.body)}>
        <Pencil size={14} aria-hidden />
      </IconButton>
    </>
  );
}

function Timeline({ projectId, number }: { projectId: string; number: number }) {
  const events = useAsync(`pull-timeline:${projectId}:${number}`, (signal) =>
    pullsApi.timeline(projectId, number, signal),
  { keep: true },
);
  const [kind, setKind] = useState("all");
  const kinds = useMemo(() => [...new Set((events.data ?? []).map((event) => event.kind))].sort(), [events.data]);
  if (events.status === "loading") return <Loading label="Loading timeline" rows={4} />;
  if (events.status === "error") {
    return <ErrorState compact title="Couldn't load the timeline" failure={events.error} onRetry={events.reload} />;
  }
  if (events.data.length === 0) return <p className="muted">No activity yet.</p>;
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
          .map((event) => (
            <li key={event.id}>
              <span className="pull-actor">{event.actor ?? "Someone"}</span> {event.text ?? event.kind}
              {event.created_at ? <time>{relativeTime(event.created_at)}</time> : null}
            </li>
          ))}
      </ol>
    </>
  );
}

/** Where this pull request sits among the ones stacked on each other. */
function Stack({ projectId, number, onOpen }: { projectId: string; number: number; onOpen?(number: number): void }) {
  const stack = useAsync(`pull-stack:${projectId}:${number}`, (signal) => pullsApi.stack(projectId, number, signal));
  const [method, setMethod] = useState("squash");
  const pushToast = useUi((state) => state.pushToast);
  if (!stack.data || stack.data.layers.length < 2) return null;
  const { layers, position } = stack.data;
  const act = (action: string) => {
    pullsApi.stackAction(projectId, number, action, action === "merge" ? method : undefined).then(
      (results) => {
        const failed = results.find((result) => !result.ok);
        pushToast(failed ? `Stopped at #${failed.number}. ${failed.message ?? ""}` : "Stack updated");
        stack.reload();
      },
      (failure: unknown) => pushToast(`Couldn't do that. ${toFailure(failure).message}`),
    );
  };
  return (
    <nav className="pull-stack" aria-label="Stack">
      <Layers size={14} aria-hidden />
      <span>
        {position + 1} of {layers.length}
      </span>
      {layers.map((layer) => (
        <button
          type="button"
          key={layer.number}
          aria-current={layer.number === number ? "true" : undefined}
          onClick={() => onOpen?.(layer.number)}
        >
          #{layer.number}
        </button>
      ))}
      <select aria-label="Stack merge method" value={method} onChange={(event) => setMethod(event.target.value)}>
        <option value="squash">Squash</option>
        <option value="merge">Merge</option>
        <option value="rebase">Rebase</option>
      </select>
      <Button small onClick={() => act("merge")}>
        Merge stack
      </Button>
      <Button small onClick={() => act("rebase")}>
        Rebase stack
      </Button>
    </nav>
  );
}

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
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const pushToast = useUi((state) => state.pushToast);
  const refreshAll = useWorkspace((state) => state.refreshAll);
  const engines = useWorkspace((state) => state.engines.data);
  const tabsId = useId();
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
  if (detail.status === "loading") {
    if (page)
      return (
        <article className="pull-page" aria-label={`Pull request #${number}`}>
          {pageHead(<i className="pull-title-skel" aria-hidden />)}
          <div className="pull-page-tabs" aria-hidden>
            {(["summary", "guide", "timeline", "code", "checks"] as const).map((entry) => (
              <button key={entry} type="button" tabIndex={-1} disabled>
                {PAGE_LABELS[entry]}
              </button>
            ))}
          </div>
          <PullPageSkeleton />
        </article>
      );
    return (
      <Loading label="Loading pull request" rows={5} />
    );
  }
  if (detail.status === "error" && !detail.data) {
    if (page) {
      return (
        <article className="pull-page" aria-label={`Pull request #${number}`}>
          {pageHead(
            <h1 tabIndex={-1} data-page-title>
              Pull request #{number}
            </h1>,
          )}
          {signedOut ? (
            <SignedOutBlock forge={forgeLabel} number={number} onSignIn={() => navigate({ name: "settings", section: "source-control" })} onLater={() => (window.history.length > 1 ? window.history.back() : navigate({ name: "pulls" }))} />
          ) : (
            <UnreachableBlock onRetry={detail.reload} />
          )}
        </article>
      );
    }
    return (
      <ErrorState title="Couldn't load this pull request" failure={detail.error} onRetry={detail.reload} />
    );
  }
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
  const items: MenuItem[] = [
    ...(page
      ? [
          { label: "Edit title", onSelect: () => setEditing(pull.title) },
          { label: "Work on this", onSelect: () => void work() },
          { label: `Open on ${forge === "The host" ? "the forge" : forge}`, disabled: connection === "offline", onSelect: () => void window.open(pull.url, "_blank", "noreferrer") },
          ...(open ? [{ label: "Close pull request", destructive: true, onSelect: () => setConfirm("close") }] : []),
        ]
      : []),
    ...(open
      ? [
          pull.draft
            ? { label: "Ready for review", onSelect: () => void update({ draft: false }, "Ready for review") }
            : { label: "Convert to draft", onSelect: () => void update({ draft: true }, "Converted to draft") },
          { label: "Update branch", onSelect: () => void act("update_branch", "Branch updated") },
          { label: "Update with rebase", onSelect: () => void act("rebase", "Branch rebased") },
          {
            label: current.maintainer_can_modify ? "Stop maintainer edits" : "Allow maintainer edits",
            onSelect: () =>
              void update({ maintainer_can_modify: !current.maintainer_can_modify }, "Maintainer edits changed"),
          },
        ]
      : []),
    { label: "Check out here", separatorBefore: true, onSelect: () => void checkout(false) },
    { label: "Check out in a worktree", onSelect: () => void checkout(true) },
    ...(pull.state === "merged"
      ? [{ label: "Revert changes", onSelect: () => void act("revert", "Revert opened") }]
      : []),
    {
      label: "Delete branch",
      destructive: true,
      separatorBefore: true,
      onSelect: () => setConfirm("delete_branch"),
    },
  ];

  const titleForm = (
    <form
      className="pull-title-form"
      onSubmit={(event) => {
        event.preventDefault();
        void update({ title: editing ?? "" }, "Title saved").then(() => setEditing(null));
      }}
    >
      <input aria-label="Title" value={editing ?? ""} autoFocus onChange={(event) => setEditing(event.target.value)} />
      <Button small tone="primary" type="submit" disabled={!(editing ?? "").trim()}>
        Save
      </Button>
      <Button small onClick={() => setEditing(null)}>
        Cancel
      </Button>
    </form>
  );
  const baseBranch = open ? (
    <input
      className="pull-base"
      aria-label="Base branch"
      defaultValue={pull.base}
      key={pull.base}
      onBlur={(event) => {
        const base = event.target.value.trim();
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
    timeline: mine?.comment_count,
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
                  <Timeline projectId={projectId} number={number} />
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
