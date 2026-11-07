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
import { Button, Field, IconButton, Pill } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { PageBar } from "../../ui/PageBar";
import { ErrorState, Loading } from "../../ui/states";
import { Markdown } from "../thread/Markdown";
import { PullCode } from "./PullCode";
import { PullConversation } from "./PullConversation";
import { PullGuide } from "./PullGuide";
import { pullsApi } from "../../api/pulls";
import { checkConnectionNow, useConnectionKind } from "../../state/connection";
import { useThreadPulls } from "../thread/pullStore";
import { agoWords, forgeName, numberLabel } from "../thread/pullStore";
import { useVisibleProjects } from "../../state/workspace";
import { PullsBanner } from "./PullStates";
import { ChecksCard, FilesCard, MergeRow, OfflineLine, PullPageSkeleton, PullStateBanner, Row, SignedOutBlock, UnreachableBlock } from "./PullPage";
import { mergeGate, pageState, reviewAskedOf, reviewWords } from "./pullModel";

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
  const checks = useAsync(`pull-checks:${projectId}:${number}`, (signal) => pullsApi.checks(projectId, number, signal));
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

function MergeDialog({
  projectId,
  detail,
  onClose,
  onMerged,
  initialMethod,
  chosen,
}: {
  projectId: string;
  detail: PullRequestDetail;
  onClose(): void;
  onMerged(): void;
  initialMethod: string;
  /** On the page the merge menu has already chosen these, so the dialog asks only for the message. */
  chosen?: { auto: boolean; deleteBranch: boolean };
}) {
  const { number } = detail.pull;
  const [method, setMethod] = useState(initialMethod);
  const [auto, setAuto] = useState(chosen?.auto ?? false);
  const [deleteBranch, setDeleteBranch] = useState(chosen?.deleteBranch ?? true);
  const [strip, setStrip] = useState(false);
  const [body, setBody] = useState(detail.body);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const merge = async () => {
    setBusy(true);
    setError(null);
    try {
      await pullsApi.merge(projectId, number, {
        method,
        auto,
        delete_branch: deleteBranch,
        strip_credits: strip,
        ...(method === "rebase" ? {} : { body }),
      });
      onMerged();
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  return (
    <Dialog
      title={`Merge #${number}?`}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} onClick={() => void merge()}>
            {auto ? "Enable auto-merge" : "Merge"}
          </Button>
        </>
      }
    >
      {chosen ? null : (
        <Field label="Merge method">
          {(props) => (
            <select {...props} value={method} onChange={(event) => setMethod(event.target.value)}>
              <option value="squash">Squash and merge</option>
              <option value="merge">Create a merge commit</option>
              <option value="rebase">Rebase and merge</option>
            </select>
          )}
        </Field>
      )}
      {method === "rebase" ? null : (
        <Field label="Commit message">
          {(props) => <textarea {...props} rows={4} value={body} onChange={(event) => setBody(event.target.value)} />}
        </Field>
      )}
      {chosen ? null : (
        <>
          <label className="check">
            <input type="checkbox" checked={auto} onChange={(event) => setAuto(event.target.checked)} />
            <span>Merge when checks pass</span>
          </label>
          <label className="check">
            <input type="checkbox" checked={deleteBranch} onChange={(event) => setDeleteBranch(event.target.checked)} />
            <span>Delete the branch</span>
          </label>
        </>
      )}
      <label className="check">
        <input type="checkbox" checked={strip} onChange={(event) => setStrip(event.target.checked)} />
        <span>Remove agent credits</span>
      </label>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
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
  onBack,
  onClose,
  onOpen,
}: {
  projectId: string;
  number: number;
  /** A page of its own, or the compact view inside a thread's side panel. */
  layout?: "page" | "panel";
  /** On the page: back to the list of pull requests. */
  onBack?(): void;
  /** In the panel: back to the thread's linked pull requests. */
  onClose?(): void;
  /** Moves to another pull request in the same stack. */
  onOpen?(number: number): void;
}) {
  const detail = useAsync(`pull:${projectId}:${number}`, (signal) => pullsApi.detail(projectId, number, signal));
  const page = layout === "page";
  // What the page layout adds beside the detail: files, checks, the details the list also reads, and who is signed in.
  const fileList = useAsync(`pull-files:${projectId}:${number}`, (signal) => (page ? pullsApi.files(projectId, number, signal) : Promise.resolve([])));
  const checkList = useAsync(`pull-checklist:${projectId}:${number}`, (signal) => (page ? pullsApi.checks(projectId, number, signal) : Promise.resolve([])));
  const extras = useAsync(`pull-extras:${projectId}:${number}`, (signal) =>
    page ? pullsApi.details({ pulls: [{ project_id: projectId, number, updated_at: null }], fresh: false }, signal) : Promise.resolve({ details: [], paused: [] }),
  );
  const forges = useAsync("pulls:forges", (signal) => pullsApi.forges(signal));
  const connection = useConnectionKind();
  const projects = useVisibleProjects();
  const linkedThread = useThreadPulls((store) => Object.entries(store.byThread).find(([, entry]) => entry.pulls?.some((one) => one.number === number))?.[0]);
  const threadTitle = useWorkspace((state) => (linkedThread ? state.threads[linkedThread]?.title : undefined));
  const [auto, setAuto] = useState(false);
  const [deleteAfter, setDeleteAfter] = useState(true);
  const [mergeMenu, setMergeMenu] = useState<{ x: number; y: number } | null>(null);
  const loadedAt = useMemo(() => (detail.data ? Date.now() : null), [detail.data]);
  const [tab, setTab] = useState<Tab>("summary");
  const [merging, setMerging] = useState(false);
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
    <PageBar>
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
      const failure = detail.error;
      return (
        <article className="pull-page" aria-label={`Pull request #${number}`}>
          {pageHead(
            <h1 tabIndex={-1} data-page-title>
              Pull request #{number}
            </h1>,
          )}
          {signedOut ? (
            <SignedOutBlock forge={forgeLabel} number={number} onSignIn={() => navigate({ name: "settings", section: "source-control" })} onLater={() => onBack?.()} />
          ) : (
            <UnreachableBlock
              forge={forgeLabel}
              number={number}
              onRetry={detail.reload}
              onCopy={() => void navigator.clipboard?.writeText(`${failure.code}: ${failure.message}`).then(() => pushToast("Copied"), () => pushToast("Couldn't copy"))}
            />
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
      detail.reload();
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
          ...(open ? [{ label: "Close pull request", onSelect: () => void act("close", "Closed") }] : []),
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
      onSelect: () => void act("delete_branch", "Branch deleted"),
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
      {merging ? (
        <MergeDialog
          projectId={projectId}
          initialMethod={mergeMethod}
          chosen={page ? { auto, deleteBranch: deleteAfter } : undefined}
          detail={current}
          onClose={() => setMerging(false)}
          onMerged={detail.reload}
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
              <Button small onClick={() => void act("close", "Closed")}>
                Close
              </Button>
              <Button
                small
                tone="primary"
                icon={<GitMerge size={14} aria-hidden />}
                disabled={mergeable === "conflicting"}
                onClick={() => setMerging(true)}
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
  const state = pageState(pull, mergeable, reviewAsked);
  const failedCheck = (checkList.data ?? []).find((check) => check.state === "fail");
  const gate = mergeGate(pull, mergeable, { offline, auto, forge: forgeLabel });
  const submitReview = (event: "approve" | "request_changes") =>
    run(pullsApi.review(projectId, number, event, ""), event === "approve" ? "Approved" : "Changes requested");
  const forgeLink = (
    <a className="btn btn-secondary btn-small pull-forge" href={pull.url} target="_blank" rel="noreferrer" aria-disabled={offline || undefined}>
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
      ) : (
        <PullStateBanner
          pull={pull}
          state={state}
          failed={failedCheck}
          actions={{
            reopen: () => void act("reopen", "Reopened"),
            ready: () => void update({ draft: false }, "Ready for review"),
            approve: () => void submitReview("approve"),
            requestChanges: () => setTab("timeline"),
            deleteBranch: () => void act("delete_branch", "Branch deleted"),
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
                <PullCode projectId={projectId} number={number} onReviewed={detail.reload} />
              ) : tab === "timeline" ? (
                <>
                  <PullConversation projectId={projectId} number={number} />
                  <Timeline projectId={projectId} number={number} />
                </>
              ) : (
                <>
                  {failing > 0 ? (
                    <PullsBanner tone="err" icon={<X size={14} aria-hidden />} actions={null}>
                      {failing} {failing === 1 ? "check" : "checks"} failed
                    </PullsBanner>
                  ) : null}
                  <Checks projectId={projectId} number={number} />
                </>
              )}
            </div>
            <div className="pull-side">{sideColumn()}</div>
          </div>
        ) : (
          <div className="pull-cols">
            <div className="pull-main">
              <div className="pull-card pull-summary">
                <b>Summary</b>
                {description}
              </div>
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
            {mergeable === "conflicting" ? "Yes" : mergeable === "mergeable" ? "None" : "\u2014"}
          </Row>
          {threadTitle ? (
            <Row name="Thread">
              <a href={`#/threads/${linkedThread}`}>{threadTitle}</a>
            </Row>
          ) : null}
        </div>
        <ChecksCard pull={pull} checks={checkList.data} />
        <MergeRow
          method={mergeMethod}
          onMethod={setMergeMethod}
          auto={auto}
          onAuto={setAuto}
          deleteBranch={deleteAfter}
          onDeleteBranch={setDeleteAfter}
          gate={current.auto_merge ? { ...gate, hint: "Merges when checks pass" } : gate}
          menu={mergeMenu}
          onMenu={setMergeMenu}
          onMerge={() => setMerging(true)}
          hidden={pull.state === "merged"}
        />
        {reviewers}
        {labels}
      </>
    );
  }
}
