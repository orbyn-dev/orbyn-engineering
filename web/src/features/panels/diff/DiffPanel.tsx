import { ChevronRight, Copy, GitPullRequest, RefreshCw, Undo2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../../api/client";
import type { FileDiff } from "../../../api/types";
import { useAsync } from "../../../lib/useAsync";
import { pullsApi } from "../../../api/pulls";
import { useWorkspace } from "../../../state/workspace";
import { useThreadLogs } from "../../../state/threadLogs";
import { useUi } from "../../../state/ui";
import { Button, IconButton } from "../../../ui/controls";
import { Empty, ErrorState, Loading } from "../../../ui/states";
import { RestoreFilesDialog } from "./RestoreFilesDialog";
import { GitActionDialog, createPullRequestPlan, useGitAvailable } from "./GitActionDialog";
import { useResolvedThreadPath } from "../../../lib/useResolvedThreadPath";
import { nextPageEnd, parsePatch } from "./diffModel";
import { Patch } from "./DiffPatch";

type DiffSource = "worktree" | "turn" | "thread" | "branch" | "commit";
const TEST_FILE = /(^|\/)(__tests__|tests?|e2e|spec)\/|\.(test|spec)\.[a-z]+$/i;

const DIFF_SOURCES: { id: DiffSource; label: string }[] = [
  { id: "worktree", label: "Working tree" },
  { id: "turn", label: "Turn" },
  { id: "thread", label: "Thread" },
  { id: "branch", label: "Branch" },
  { id: "commit", label: "Commit" },
];

const STATUS_LETTER: Record<string, string> = { added: "A", deleted: "D", renamed: "R", modified: "M" };

function FileRow({
  file,
  open,
  find,
  threadId,
  origin,
  onToggle,
}: {
  file: FileDiff;
  open: boolean;
  find: string;
  threadId: string;
  origin: string;
  onToggle(): void;
}) {
  const pushToast = useUi((state) => state.pushToast);
  return (
    <li className="diff-file" data-diff-file={file.path}>
      <div className="diff-file-head">
        <button type="button" className="diff-file-row" aria-expanded={open} onClick={onToggle}>
          <ChevronRight size={16} className="chev" aria-hidden />
          <span className="diff-status" data-status={file.status} title={file.status}>
            {STATUS_LETTER[file.status] ?? "?"}
          </span>
          <span className="diff-path" title={file.path}>
            {file.path}
          </span>
          <span className="diff-add">+{file.additions}</span>
          <span className="diff-del">-{file.deletions}</span>
        </button>
        <IconButton
          label={`Copy path ${file.path}`}
          onClick={() => void navigator.clipboard?.writeText(file.path).then(() => pushToast("Path copied"))}
        >
          <Copy size={14} aria-hidden />
        </IconButton>
      </div>
      {open ? <Patch patch={file.patch} find={find} path={file.path} threadId={threadId} origin={origin} /> : null}
    </li>
  );
}

/** Unified or split, the same stored choice as Settings › Diffs. */
function DiffView() {
  const layout = useUi((state) => state.diffLayout);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <div className="diff-view segment" role="radiogroup" aria-label="Diff view">
      {([["stacked", "Unified"], ["split", "Split"]] as const).map(([id, label]) => (
        <button key={id} type="button" role="radio" aria-checked={layout === id} onClick={() => setPrefs({ diffLayout: id })}>
          {label}
        </button>
      ))}
    </div>
  );
}

function DiffOptions() {
  const collapsed = useUi((state) => state.diffFilesCollapsed);
  const ignoreSpace = useUi((state) => state.diffIgnoreWhitespace);
  const hideTests = useUi((state) => state.diffHideTests);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <div className="diff-options">
      <label>
        <input type="checkbox" checked={collapsed} onChange={(event) => setPrefs({ diffFilesCollapsed: event.target.checked })} />
        Collapse files
      </label>
      <label>
        <input type="checkbox" checked={ignoreSpace} onChange={(event) => setPrefs({ diffIgnoreWhitespace: event.target.checked })} />
        Ignore whitespace
      </label>
      <label>
        <input type="checkbox" checked={hideTests} onChange={(event) => setPrefs({ diffHideTests: event.target.checked })} />
        Hide tests
      </label>
    </div>
  );
}

/** A Jujutsu checkout's working-copy change, as the installed `jj` describes it. */
function JjChange({ threadId, version }: { threadId: string; version: number }) {
  const status = useAsync(`jj:${threadId}:${version}`, (signal) => api.jjStatus(threadId, signal));
  if (status.status === "error") {
    return <p className="jj-change" role="note">Jujutsu: {status.error.message}</p>;
  }
  const change = status.data;
  if (!change) return null;
  return (
    <p className="jj-change" role="note" aria-label="Jujutsu change">
      <span>jj change <code>{change.change_id}</code></span>
      <span> commit <code>{change.commit_id}</code></span>
      {change.bookmarks.length > 0 ? <span> on {change.bookmarks.join(", ")}</span> : null}
      <span> · {change.description || "(no description)"}</span>
      {change.conflict ? <span className="diff-del"> · conflicted</span> : null}
    </p>
  );
}

export function DiffPanel({ threadId }: { threadId: string }) {
  const version = useThreadLogs((state) => state.diffVersion[threadId] ?? 0);
  const [source, setSource] = useState<DiffSource>("worktree");
  const checkpoints = useAsync(`checkpoints:${threadId}:${source === "turn" ? version : "off"}`, (signal) =>
    source === "turn" ? api.checkpoints(threadId, signal) : Promise.resolve([]),
  );
  const turns = checkpoints.data ?? [];
  const [pickedRun, setPickedRun] = useState<string | null>(null);
  const latestRun = (pickedRun && turns.some((turn) => turn.run_id === pickedRun) ? pickedRun : turns.at(-1)?.run_id) ?? null;
  const thread = useWorkspace((state) => state.threads[threadId]);
  const linkedPulls = useAsync(`diff-linked:${threadId}`, (signal) => pullsApi.linked(threadId, signal));
  const gitAvailable = useGitAvailable(threadId);
  const canCreate = gitAvailable && linkedPulls.status === "ready" && linkedPulls.data.length === 0;
  const [restoring, setRestoring] = useState(false);
  const [creating, setCreating] = useState(false);
  const commits = useAsync(`commits:${threadId}:${source}`, (signal) =>
    source === "commit" ? api.commits(threadId, signal) : Promise.resolve([]),
  );
  const [picked, setPicked] = useState<string | null>(null);
  const commitReveal = useUi((state) => state.commitReveal);
  useEffect(() => {
    if (!commitReveal) return;
    setSource("commit");
    setPicked(commitReveal.sha);
  }, [commitReveal]);
  const sha = picked ?? commits.data?.[0]?.sha ?? null;
  const base = useWorkspace((state) => state.threads[threadId]?.worktree?.base ?? "main");
  const scope =
    source === "turn"
      ? latestRun
        ? `turn&run=${encodeURIComponent(latestRun)}`
        : null
      : source === "branch"
        ? `branch&base=${encodeURIComponent(base)}`
        : source === "commit"
          ? sha
            ? `commit&sha=${encodeURIComponent(sha)}`
            : null
          : source;
  // Which diff a comment was made on, so it shows only there.
  const origin =
    source === "turn" ? `diff:turn:${latestRun ?? ""}`
      : source === "branch" ? `diff:branch:${base}`
        : source === "commit" ? `diff:commit:${sha ?? ""}`
          : `diff:${source}`;
  const diff = useAsync(`diff:${threadId}:${scope ?? "none"}`, (signal) =>
    scope ? api.diff(threadId, signal, scope) : Promise.resolve([]),
  );
  const { reload } = diff;
  // Whether a pull request has anything to carry is the branch's question, not this view's:
  // a clean working tree after a commit, or Hide tests filtering a test-only change, still
  // leaves commits ahead of the base. The branch diff is read only where a pull could be made.
  const branchAhead = useAsync(`diff-branch-any:${threadId}:${base}:${canCreate}`, (signal) =>
    canCreate ? api.diff(threadId, signal, `branch&base=${encodeURIComponent(base)}`) : Promise.resolve([]),
  );
  const [toggled, setToggled] = useState<ReadonlySet<string>>(new Set());
  const [find, setFind] = useState("");
  const collapsed = useUi((state) => state.diffFilesCollapsed);
  const hideTests = useUi((state) => state.diffHideTests);
  const reveal = useUi((state) => state.diffReveal);
  const resolved = useResolvedThreadPath(threadId, reveal ? { ...reveal, basis: "repository" } : null);
  const consume = useUi((state) => state.consumeDiffReveal);
  const [loaded, setLoaded] = useState(0);
  const shownFiles = useMemo(() => {
    const every = diff.data ?? [];
    return hideTests ? every.filter((file) => !TEST_FILE.test(file.path)) : every;
  }, [diff.data, hideTests]);
  const shownCount = Math.max(loaded, nextPageEnd(shownFiles, 0));
  // A new diff starts again from its first page.
  useEffect(() => setLoaded(0), [scope, hideTests]);
  const root = useRef<HTMLDivElement>(null);
  const more = useRef<HTMLLIElement>(null);

  useEffect(() => {
    if (!reveal) return;
    if (reveal.threadId !== threadId || useWorkspace.getState().threads[threadId]?.cwd !== reveal.cwd) consume(reveal.nonce);
  }, [reveal, threadId, resolved.key, consume]);

  useEffect(() => {
    if (version > 0) reload();
  }, [version, reload]);

  useEffect(() => {
    const path = resolved.value?.repository_path;
    if (!path || !reveal || useUi.getState().diffReveal?.nonce !== reveal.nonce
      || useWorkspace.getState().threads[threadId]?.cwd !== reveal.cwd) return;
    // A file past the loaded pages loads first, then scrolls into view.
    const at = shownFiles.findIndex((file) => file.path === path);
    if (at >= shownCount) { setLoaded(at + 1); return; }
    const target = root.current?.querySelector(`[data-diff-file="${CSS.escape(path)}"]`);
    if (target) { target.scrollIntoView({ block: "start" }); consume(reveal.nonce); }
  }, [reveal, resolved.value, diff.data, consume, threadId, shownFiles, shownCount]);

  // Loading the next page as the end of the list scrolls into view.
  useEffect(() => {
    const sentinel = more.current;
    if (!sentinel || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) setLoaded((count) => nextPageEnd(shownFiles, Math.max(count, shownCount)));
    }, { rootMargin: "400px" });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [shownFiles, shownCount]);

  if (diff.status === "loading") return <Loading label="Loading diff" />;
  if (diff.status === "error" && !diff.data) {
    return <ErrorState title="Couldn't load the diff" failure={diff.error} onRetry={reload} />;
  }
  const files = shownFiles;
  const page = files.slice(0, shownCount);
  const needle = find.trim().toLowerCase();
  // Find covers the files loaded so far, and says so while more remain.
  const hits = needle
    ? page.reduce((sum, file) => sum + parsePatch(file.patch).filter((line) => line.text.toLowerCase().includes(needle)).length, 0)
    : 0;
  const added = files.reduce((sum, file) => sum + file.additions, 0);
  const removed = files.reduce((sum, file) => sum + file.deletions, 0);

  // Nothing changed means nothing to open a pull request for; the empty state offers Files instead.
  const offerPull = canCreate && ((diff.data?.length ?? 0) > 0 || (branchAhead.data?.length ?? 0) > 0);
  return (
    <div ref={root}>
      <JjChange threadId={threadId} version={version} />
      <div className="diff-sources segment" role="radiogroup" aria-label="Diff source">
        {DIFF_SOURCES.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="radio"
            aria-checked={source === entry.id}
            onClick={() => setSource(entry.id)}
          >
            {entry.label}
          </button>
        ))}
      </div>
      {source === "commit" ? (
        <select
          className="diff-commit"
          aria-label="Commit"
          value={sha ?? ""}
          onChange={(event) => setPicked(event.target.value)}
        >
          {(commits.data ?? []).map((commit) => (
            <option key={commit.sha} value={commit.sha}>
              {commit.subject}
            </option>
          ))}
        </select>
      ) : null}
      <DiffView />
      {source === "turn" && turns.length > 0 ? (
        <select
          className="diff-commit"
          aria-label="Turn"
          value={latestRun ?? ""}
          onChange={(event) => setPickedRun(event.target.value)}
        >
          {turns.map((turn, at) => (
            <option key={turn.run_id} value={turn.run_id}>
              {`Turn ${at + 1} of ${turns.length}`}
            </option>
          ))}
        </select>
      ) : null}
      <DiffOptions />
      <div className="diff-find">
        <input
          type="search"
          aria-label="Find in diff"
          placeholder="Find in diff"
          value={find}
          onChange={(event) => setFind(event.target.value)}
        />
        {needle ? <output>{hits === 1 ? "1 match" : `${hits} matches`}</output> : null}
        {needle && page.length < files.length ? (
          <p className="muted" role="note">Find covers the {page.length} of {files.length} files loaded so far. Scroll to load more.</p>
        ) : null}
      </div>
      <div className="panel-bar">
        <span>
          {files.length === 1 ? "1 file" : `${files.length} files`}
          {files.length > 0 ? (
            <>
              {" "}
              <span className="diff-add">+{added}</span> <span className="diff-del">-{removed}</span>
            </>
          ) : null}
        </span>
        <IconButton label="Refresh diff" onClick={reload} disabled={diff.refreshing}>
          <RefreshCw size={16} className={diff.refreshing ? "spin" : undefined} aria-hidden />
        </IconButton>
      </div>
      {diff.status === "error" ? (
        <ErrorState compact title="Couldn't refresh the diff" failure={diff.error} onRetry={reload} />
      ) : null}
      {files.length === 0 ? (
        <Empty title="No changes yet">
          <p className="t3">Files the agent edits show up here, with a diff for each one.</p>
          <Button small tone="quiet" onClick={() => useUi.getState().togglePanel("files")}>
            Open Files
          </Button>
        </Empty>
      ) : (
        <ul className="diff-files">
          {page.map((file) => (
            <FileRow
              key={file.path}
              file={file}
              find={needle}
              threadId={threadId}
              origin={origin}
              open={collapsed === toggled.has(file.path)}
              onToggle={() => {
                const next = new Set(toggled);
                if (!next.delete(file.path)) next.add(file.path);
                setToggled(next);
              }}
            />
          ))}
          {page.length < files.length ? (
            <li ref={more} className="diff-more">
              <Button small tone="quiet" onClick={() => setLoaded(nextPageEnd(files, page.length))}>
                Load more files ({files.length - page.length} left)
              </Button>
            </li>
          ) : null}
        </ul>
      )}
      {thread && thread.active_run === null && ((source === "turn" && latestRun) || offerPull) ? (
        <div className="diff-foot">
          {source === "turn" && latestRun ? (
            <Button small tone="quiet" icon={<Undo2 size={14} aria-hidden />} onClick={() => setRestoring(true)}>
              Restore files to before this response
            </Button>
          ) : null}
          {offerPull ? (
            <Button small tone="quiet" icon={<GitPullRequest size={14} aria-hidden />} onClick={() => setCreating(true)}>
              Create pull request
            </Button>
          ) : null}
        </div>
      ) : null}
      {restoring && latestRun ? <RestoreFilesDialog threadId={threadId} runId={latestRun} onClose={() => setRestoring(false)} /> : null}
      {creating && thread ? <GitActionDialog thread={thread} plan={createPullRequestPlan()} onClose={() => setCreating(false)} /> : null}
    </div>
  );
}
