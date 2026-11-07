import { Check, ChevronDown, CloudOff, GitPullRequest, RefreshCw, TriangleAlert, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useCommand } from "../../app/shortcuts";
import { pullsApi } from "../../api/pulls";
import type { PullRequest, Thread } from "../../api/types";
import { useWaited } from "../../lib/motion";
import { toFailure } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { useNow } from "../../lib/useNow";
import { useConnectionKind } from "../../state/connection";
import { navigate } from "../../state/route";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { Button, IconButton } from "../../ui/controls";
import { ToolbarPopover } from "../pulls/ToolbarPopover";
import { GitActionDialog, createPullRequestPlan, useGitAvailable } from "./GitControl";
import { LinkPullDialog } from "./LinkPullDialog";
import {
  agoWords,
  checksSummary,
  chipDot,
  chipLabel,
  entryOf,
  forgeName,
  readyToMerge,
  stateWord,
  useThreadPulls,
  WATCH_TIP,
} from "./pullStore";
import "../pulls/pullsToolbar.css";
import "./pullChip.css";

/** Reads the thread's linked pull requests once it shows, and again whenever it changes. */
function useLinked(thread: Thread) {
  const load = useThreadPulls((state) => state.load);
  const { id, project_id: projectId } = thread;
  useEffect(() => {
    void load({ id, project_id: projectId });
  }, [load, id, projectId]);
  return useThreadPulls((state) => entryOf(state, thread.id));
}

function Skeleton() {
  return (
    <div className="pr-row" data-skeleton aria-busy="true">
      <span className="pr-title">
        <span className="pr-skel" style={{ width: 220, height: 12 }} />
      </span>
      <span className="pr-watch">
        <span className="pr-skel" style={{ width: 30, height: 18, borderRadius: 9 }} />
      </span>
      <span className="pr-meta">
        <span className="pr-skel" style={{ width: 170, height: 10 }} />
      </span>
    </div>
  );
}

function Row({ thread, pull, onOpen }: { thread: Thread; pull: PullRequest; onOpen(): void }) {
  const entry = useThreadPulls((state) => entryOf(state, thread.id));
  const setWatch = useThreadPulls((state) => state.setWatch);
  const pushToast = useUi((state) => state.pushToast);
  const offline = useConnectionKind() === "offline";
  const load = useThreadPulls((state) => state.load);
  const word = stateWord(pull);
  const ready = readyToMerge(pull, entry.mergeable[pull.number]);
  const summary = checksSummary(pull);
  const project = useWorkspace((state) => state.projects.data?.find((one) => one.id === thread.project_id)?.name);
  const watching = entry.watched.includes(pull.number);
  const open = () => {
    onOpen();
    navigate({ name: "pulls", project: thread.project_id, number: pull.number });
  };
  return (
    <div
      className="pr-row"
      role="link"
      tabIndex={-1}
      aria-label={`#${pull.number} ${pull.title}`}
      onClick={(event) => {
        if (!(event.target as HTMLElement).closest("button, input, label")) open();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) open();
      }}
    >
      <span className="pr-title">
        <span className="pr-num">#{pull.number}</span>
        <b>{pull.title}</b>
      </span>
      <label className="pr-watch" data-disabled={offline || undefined} title={WATCH_TIP}>
        <span>Watch</span>
        <input
          type="checkbox"
          role="switch"
          aria-label="Watch"
          checked={watching}
          disabled={offline}
          onChange={(event) => {
            setWatch(thread.id, pull.number, event.target.checked).catch((failure: unknown) =>
              pushToast(`Couldn't do that. ${toFailure(failure).message}`),
            );
          }}
        />
      </label>
      <span className="pr-meta">
        {word ? <span className="pr-state">{word}</span> : <span className="pr-quiet">Draft</span>}
        {ready ? <span className="pr-ready">Ready to merge</span> : null}
        {summary ? (
          <span className="pr-checks" data-tone={summary.tone}>
            {summary.tone === "pass" ? <Check size={12} aria-hidden /> : null}
            {summary.tone === "fail" ? <X size={12} aria-hidden /> : null}
            {summary.tone === "running" ? <i className="spin" aria-hidden /> : null}
            {summary.text}
          </span>
        ) : null}
        <span className="pr-branch">{[project, pull.head].filter(Boolean).join(" · ")}</span>
        <button
          type="button"
          className="pr-unlink"
          onClick={() => {
            pullsApi.unlink(thread.id, pull.number).then(
              () => load(thread),
              (failure: unknown) => pushToast(`Couldn't do that. ${toFailure(failure).message}`),
            );
          }}
        >
          Unlink
        </button>
      </span>
    </div>
  );
}

/**
 * The thread's linked pull requests: a row each with its status, checks, and a Watch switch, then
 * Refresh and when it was read. The chip's popover and the details card's Pull requests row both
 * show this, so a change in one is a change in the other.
 */
export function PullList({ thread, onOpen, focusFirst, sheet = false }: { thread: Thread; onOpen(): void; focusFirst?: boolean; sheet?: boolean }) {
  const entry = useThreadPulls((state) => entryOf(state, thread.id));
  const refresh = useThreadPulls((state) => state.refresh);
  const load = useThreadPulls((state) => state.load);
  const offline = useConnectionKind() === "offline";
  const now = useNow(true, 30_000);
  const [linking, setLinking] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const waited = useWaited(entry.busy && entry.pulls === null);
  const updating = useWaited(entry.busy);
  const pulls = entry.pulls ?? [];
  useEffect(() => {
    if (focusFirst) root.current?.querySelector<HTMLElement>(".pr-row")?.focus();
  }, [focusFirst]);
  // Opening the list reads it again, so what it shows is current.
  useEffect(() => {
    void load(thread);
    // Once, as the list opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const move = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = [...(root.current?.querySelectorAll<HTMLElement>(".pr-row:not([data-skeleton])") ?? [])];
    const at = rows.findIndex((row) => row === document.activeElement || row.contains(document.activeElement));
    const next = rows[at + (event.key === "ArrowDown" ? 1 : -1)];
    if (!next) return;
    event.preventDefault();
    next.focus();
  };
  const ago = entry.loadedAt === null ? "" : agoWords(entry.loadedAt, now);
  const title = pulls.length === 1 ? null : pulls.length > 1 ? `${pulls.length} linked pull requests` : "Linked pull requests";
  const failedBy = forgeName(pulls[0]?.url);
  return (
    <div className="pr-pop" ref={root} data-sheet={sheet || undefined} onKeyDown={move}>
      {title ? <div className="pr-head">{title}</div> : null}
      {offline && entry.loadedAt !== null ? (
        <div className="pr-note" role="status">
          <CloudOff size={13} aria-hidden />
          <span>You’re offline. Showing what was loaded {ago}.</span>
        </div>
      ) : entry.failed && entry.loadedAt !== null ? (
        <div className="pr-note" role="status">
          <TriangleAlert size={13} aria-hidden />
          <span>
            {failedBy} didn’t answer. Showing what was loaded {ago}.
          </span>
          <Button small onClick={() => void refresh(thread)}>
            Try again
          </Button>
        </div>
      ) : null}
      {entry.pulls === null ? (
        waited ? (
          <>
            <Skeleton />
            <Skeleton />
            <Skeleton />
          </>
        ) : null
      ) : (
        pulls.map((pull) => <Row key={pull.number} thread={thread} pull={pull} onOpen={onOpen} />)
      )}
      <div className="pr-foot">
        <Button small className="pr-refresh" disabled={offline || entry.busy} onClick={() => void refresh(thread)}>
          <RefreshCw size={12} aria-hidden />
          Refresh
        </Button>
        <button type="button" className="pr-link" onClick={() => setLinking(true)}>
          Link a pull request
        </button>
        <span className="pr-updated">{updating ? "Updating…" : entry.loadedAt === null ? "" : `Updated ${ago}`}</span>
      </div>
      {linking ? <LinkPullDialog thread={thread} onClose={() => setLinking(false)} onLinked={() => void load(thread)} /> : null}
    </div>
  );
}

/** The header's pull request chip. A thread with none linked has no chip at all. */
export function PullChip({ thread }: { thread: Thread }) {
  const entry = useLinked(thread);
  const [open, setOpen] = useState<{ anchor: HTMLElement; sheet: boolean } | null>(null);
  const layout = useLayoutWidth();
  const sheet = layout === "phone";
  const pulls = entry.pulls ?? [];
  const pushToast = useUi((state) => state.pushToast);
  useCommand("pullRequest.copyNumber", () => {
    const first = pulls[0];
    if (!first) return false;
    void navigator.clipboard?.writeText(`#${first.number}`).then(() => pushToast(`Copied #${first.number}`));
  });
  if (pulls.length === 0) return null;
  const heading = pulls.length === 1 ? "Pull requests" : `${pulls.length} linked pull requests`;
  return (
    <>
      <button
        type="button"
        className="pr-chip"
        aria-label="Pull requests"
        aria-haspopup="dialog"
        aria-expanded={open !== null}
        data-pressed={open !== null || undefined}
        onClick={(event) => setOpen(open ? null : { anchor: event.currentTarget, sheet })}
      >
        <GitPullRequest size={14} aria-hidden />
        {chipLabel(pulls)}
        <span className="pr-dot" data-tone={chipDot(pulls)} aria-hidden />
      </button>
      {open ? (
        <ToolbarPopover anchor={open.anchor} label="Pull requests" layout={open.sheet ? "sheet" : "popover"} width={392} closeNow={open.sheet && !sheet} onClose={() => setOpen(null)}>
          {(dismiss) =>
            open.sheet ? (
              <>
                <div className="pulls-sheet-top" data-sheet-handle>
                  <span className="pulls-grab" aria-hidden />
                  <div className="pulls-sheet-head">
                    <b>{heading}</b>
                    <IconButton label="Close" onClick={dismiss}>
                      <X size={14} aria-hidden />
                    </IconButton>
                  </div>
                </div>
                <div className="pulls-sheet-body">
                  <PullList thread={thread} sheet onOpen={dismiss} />
                </div>
              </>
            ) : (
              <PullList thread={thread} focusFirst onOpen={dismiss} />
            )
          }
        </ToolbarPopover>
      ) : null}
    </>
  );
}

/** The details card's Pull requests row: how many, and the same list opened in place. */
export function PullsRow({ thread }: { thread: Thread }) {
  const entry = useLinked(thread);
  const [expanded, setExpanded] = useState(false);
  const [creating, setCreating] = useState(false);
  const [linking, setLinking] = useState(false);
  const gitAvailable = useGitAvailable(thread.id);
  const load = useThreadPulls((state) => state.load);
  if (entry.pulls === null) return null;
  const count = entry.pulls.length;
  if (count === 0) {
    return (
      <div className="detail-row detail-row-static">
        <GitPullRequest size={16} aria-hidden />
        <span className="detail-dim">No pull request yet</span>
        {gitAvailable ? (
          <Button small onClick={() => setCreating(true)}>
            Open a pull request
          </Button>
        ) : null}
        <Button small onClick={() => setLinking(true)}>
          Link a pull request
        </Button>
        {creating ? <GitActionDialog thread={thread} plan={createPullRequestPlan()} onClose={() => setCreating(false)} /> : null}
        {linking ? <LinkPullDialog thread={thread} onClose={() => setLinking(false)} onLinked={() => void load(thread)} /> : null}
      </div>
    );
  }
  return (
    <>
      <button type="button" className="detail-row" aria-expanded={expanded} aria-label="Pull requests" onClick={() => setExpanded(!expanded)}>
        <GitPullRequest size={16} aria-hidden />
        <span>{count === 1 ? "1 pull request" : `${count} pull requests`}</span>
        <ChevronDown size={14} aria-hidden data-open={expanded || undefined} className="detail-chevron" />
      </button>
      {expanded ? <PullList thread={thread} onOpen={() => undefined} /> : null}
    </>
  );
}
