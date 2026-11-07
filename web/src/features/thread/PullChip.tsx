import { Check, CloudOff, GitPullRequest, Plus, RefreshCw, TriangleAlert, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { useCommand } from "../../app/shortcuts";
import { ApiFailure } from "../../api/client";
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
import { DRow } from "./DetailRows";
import { captureFocusReturn } from "../../ui/focusReturn";
import { ToolbarPopover } from "../pulls/ToolbarPopover";
import { GitActionDialog, createPullRequestPlan, useGitAvailable } from "./GitControl";
import {
  agoWords,
  checksSummary,
  chipDot,
  chipLabel,
  entryOf,
  forgeName,
  numberLabel,
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
  const retain = useThreadPulls((state) => state.retain);
  const { id, project_id: projectId } = thread;
  // The entry stays while any consumer (the chip, the details row) shows it, and goes with the last.
  useEffect(() => retain(id), [retain, id]);
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

/** A pull request number typed in: links it to the thread, or says there is no such one. */
export function LinkForm({ thread, host = "The host", onDone, onCancel }: { thread: Thread; host?: string; onDone(): void; onCancel(): void }) {
  const load = useThreadPulls((state) => state.load);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const number = Number(/^#?(\d{1,16})$/.exec(value.trim())?.[1]);
    if (!Number.isSafeInteger(number) || number < 1) {
      setError("Enter a pull request number.");
      return;
    }
    setBusy(true);
    setError(null);
    pullsApi.link(thread.id, number).then(
      () => {
        void load(thread);
        onDone();
      },
      (failure: unknown) => {
        setBusy(false);
        // A 404 says there is no such pull request. Another 4xx is the service's own answer and is shown as it is;
        // only a server error or no answer at all reads as the host not answering, which Retry can help.
        const status = failure instanceof ApiFailure ? failure.status : 0;
        const refused = status >= 400 && status < 500 && status !== 404 && status !== 408 && status !== 429;
        setFailed(status !== 404 && !refused);
        setError(status === 404 ? `No pull request #${number} in this repository.` : refused ? toFailure(failure).message : `${host} didn’t answer.`);
      },
    );
  };
  return (
    <form className="pr-link-form" onSubmit={submit}>
      <div className="pr-head">Link a pull request</div>
      <label className="pr-link-label">
        <span>Pull request number</span>
        <span className="pr-link-field" data-invalid={error ? "" : undefined}>
          <span aria-hidden>#</span>
          <input aria-label="Pull request number" inputMode="numeric" maxLength={17} value={value} autoFocus onChange={(event) => setValue(event.target.value)} />
        </span>
      </label>
      {error ? (
        <p className="pr-link-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="pr-link-acts">
        <Button small onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" small tone="primary" disabled={busy}>
          {failed ? "Retry" : "Link"}
        </Button>
      </div>
    </form>
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
  const unlink = async (from: HTMLElement) => {
    // Focus goes to the next row's first control, else the previous row's, else "Link a pull request…";
    // the last row takes the chip with it, so then it goes to a stable element, the Details button.
    const row = from.closest(".pr-row");
    const sibling = (step: "nextElementSibling" | "previousElementSibling") =>
      row?.[step]?.matches(".pr-row") ? row[step]!.querySelector<HTMLElement>(".pr-open") : null;
    const last = (entry.pulls?.length ?? 0) <= 1;
    const target = last
      ? document.querySelector<HTMLElement>("[data-details-toggle]")
      : (sibling("nextElementSibling") ?? sibling("previousElementSibling") ?? row?.closest(".pr-pop")?.querySelector<HTMLElement>(".pr-link") ?? null);
    // Inside an open list the shared rule would refuse the target, so a row or "Link…" takes focus directly.
    const inside = target !== null && !!target.closest(".pr-pop");
    const back = inside ? null : captureFocusReturn(target);
    try {
      await pullsApi.unlink(thread.id, pull.number);
      await load(thread);
      if (back) back.restore();
      else target?.focus();
    } catch (failure) {
      pushToast(`Couldn't do that. ${toFailure(failure).message}`);
    }
  };
  return (
    <div className="pr-row">
      <button
        type="button"
        className="pr-open"
        aria-label={`${numberLabel(pull)} ${pull.title}`}
        onClick={() => {
          onOpen();
          navigate({ name: "pulls", project: thread.project_id, number: pull.number });
        }}
      >
        <span className="pr-title">
          <span className="pr-num">{numberLabel(pull)}</span>
          <b>{pull.title}</b>
        </span>
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
        </span>
      </button>
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
      <button type="button" className="pr-unlink" aria-label={`Unlink ${numberLabel(pull)}`} onClick={(event) => void unlink(event.currentTarget)}>
        Unlink
      </button>
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
  const loaded = entry.pulls !== null;
  // Leaving the form (Cancel, or a pull request linked) puts focus back on "Link a pull request…".
  const wasLinking = useRef(false);
  useEffect(() => {
    if (linking) wasLinking.current = true;
    else if (wasLinking.current) {
      wasLinking.current = false;
      root.current?.querySelector<HTMLElement>(".pr-link")?.focus();
    }
  }, [linking]);
  useEffect(() => {
    if (focusFirst) root.current?.querySelector<HTMLElement>(".pr-open, .pr-retry")?.focus();
  }, [focusFirst, loaded]);
  // Opening the list reads it again, so what it shows is current.
  useEffect(() => {
    void load(thread);
    // Once, as the list opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const move = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if ((event.target as HTMLElement).closest("input[type=text], input:not([type])")) return;
    const rows = [...(root.current?.querySelectorAll<HTMLElement>(".pr-open") ?? [])];
    const at = rows.findIndex((row) => row.closest(".pr-row")?.contains(document.activeElement));
    const next = rows[at + (event.key === "ArrowDown" ? 1 : -1)];
    if (!next) return;
    event.preventDefault();
    next.focus();
  };
  const ago = entry.loadedAt === null ? "" : agoWords(entry.loadedAt, now);
  const title = pulls.length === 1 ? null : pulls.length > 1 ? `${pulls.length} linked pull requests` : "Linked pull requests";
  const host = forgeName(pulls[0]?.url);
  const firstFailed = entry.pulls === null && entry.failed;
  if (linking) return <div className="pr-pop" ref={root} data-sheet={sheet || undefined}><LinkForm thread={thread} host={host} onDone={() => setLinking(false)} onCancel={() => setLinking(false)} /></div>;
  return (
    <div className="pr-pop" ref={root} data-sheet={sheet || undefined} onKeyDown={move}>
      {title ? <div className="pr-head">{title}</div> : null}
      {offline && entry.loadedAt !== null ? (
        <div className="pr-note" role="status">
          <CloudOff size={13} aria-hidden />
          <span>You’re offline. Showing what was loaded {ago}.</span>
        </div>
      ) : firstFailed ? (
        <div className="pr-note" role="alert">
          <TriangleAlert size={13} aria-hidden />
          <span>{host} didn’t answer.</span>
          <Button small className="pr-retry" disabled={entry.busy} onClick={() => void load(thread)}>
            Retry
          </Button>
        </div>
      ) : entry.failed && entry.loadedAt !== null ? (
        <div className="pr-note" role="status">
          <TriangleAlert size={13} aria-hidden />
          <span>
            {host} didn’t answer. Showing what was loaded {ago}.
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
      <button type="button" className="pr-link" aria-expanded={linking} onClick={() => setLinking(true)}>
        <Plus size={14} aria-hidden />
        Link a pull request…
      </button>
      <div className="pr-foot">
        <Button small className="pr-refresh" disabled={offline || entry.busy} onClick={() => void refresh(thread)}>
          <RefreshCw size={12} aria-hidden />
          Refresh
        </Button>
        <span className="pr-updated">{updating ? "Updating…" : entry.loadedAt === null ? "" : `Updated ${ago}`}</span>
      </div>
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
  const failed = entry.pulls === null && entry.failed;
  const pushToast = useUi((state) => state.pushToast);
  useCommand("pullRequest.copyNumber", () => {
    const first = pulls[0];
    if (!first) return false;
    void navigator.clipboard?.writeText(`#${first.number}`).then(() => pushToast(`Copied #${first.number}`));
  });
  if (pulls.length === 0 && !failed) return null;
  const heading = pulls.length === 1 ? "Pull requests" : pulls.length > 1 ? `${pulls.length} linked pull requests` : "Linked pull requests";
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
        {failed ? <TriangleAlert size={14} aria-hidden /> : <GitPullRequest size={14} aria-hidden />}
        {failed ? "PRs" : chipLabel(pulls)}
        {failed ? null : <span className="pr-dot" data-tone={chipDot(pulls)} aria-hidden />}
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
                  <PullList thread={thread} sheet focusFirst onOpen={dismiss} />
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

/** The details card's Pull requests row: the number and state, or how many, and the same list opened in place. */
export function PullsRow({ thread }: { thread: Thread }) {
  const entry = useLinked(thread);
  const load = useThreadPulls((state) => state.load);
  const [expanded, setExpanded] = useState(false);
  const [creating, setCreating] = useState(false);
  const [linking, setLinking] = useState<HTMLElement | null>(null);
  const gitAvailable = useGitAvailable(thread.id);
  const offline = useConnectionKind() === "offline";
  const failed = entry.pulls === null && entry.failed;
  const loading = entry.pulls === null && !failed;
  const waited = useWaited(loading);
  const pulls = entry.pulls ?? [];
  const count = pulls.length;
  // A pull request linked from the empty row turns it into the count row: focus goes there.
  const linkedFromEmpty = useRef(false);
  const rowButton = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (count > 0 && linkedFromEmpty.current) {
      linkedFromEmpty.current = false;
      captureFocusReturn(rowButton.current?.querySelector("button") ?? null).restore();
    }
  }, [count]);
  const icon = <GitPullRequest size={16} aria-hidden />;
  if (loading) return <DRow icon={icon} label="Pull requests" loading waited={waited} skeleton={70} />;
  if (failed) {
    return (
      <DRow
        icon={icon}
        label="Pull requests"
        value={<span className="t3">{offline ? "Offline" : "GitHub unreachable"}</span>}
        extra={
          offline ? null : (
            <Button small tone="quiet" onClick={() => void load(thread)}>
              Try again
            </Button>
          )
        }
      />
    );
  }
  if (count === 0) {
    return (
      <>
        <DRow
          icon={icon}
          label="Pull requests"
          value={<span className="t3">No pull request yet</span>}
          sub={
            <span className="dlinks">
              {gitAvailable ? (
                <>
                  <button type="button" className="dlink" onClick={() => setCreating(true)}>
                    Open a pull request
                  </button>
                  <span> · </span>
                </>
              ) : null}
              <button type="button" className="dlink" aria-haspopup="dialog" onClick={(event) => setLinking(linking ? null : event.currentTarget)}>
                Link by number
              </button>
            </span>
          }
        />
        {creating ? <GitActionDialog thread={thread} plan={createPullRequestPlan()} onClose={() => setCreating(false)} /> : null}
        {linking ? (
          <ToolbarPopover anchor={linking} label="Link by number" width={260} onClose={() => setLinking(null)}>
            {(dismiss) => <LinkForm thread={thread} onDone={() => { linkedFromEmpty.current = true; dismiss(); }} onCancel={dismiss} />}
          </ToolbarPopover>
        ) : null}
      </>
    );
  }
  const first = pulls[0];
  const word = count === 1 && first ? (stateWord(first) ?? "Draft") : null;
  return (
    <div ref={rowButton}>
      <DRow
        icon={icon}
        label="Pull requests"
        ariaLabel="Pull requests"
        expanded={expanded}
        onClick={() => setExpanded(!expanded)}
        value={
          count === 1 && first ? (
            <>
              <span className="t2">{numberLabel(first)}</span>
              <span className="pillx">{word}</span>
            </>
          ) : (
            <span className="t2">{count} pull requests</span>
          )
        }
      >
        {expanded ? <PullList thread={thread} onOpen={() => undefined} /> : null}
      </DRow>
    </div>
  );
}
