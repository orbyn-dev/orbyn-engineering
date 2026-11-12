import { FileDiff as DiffIcon, GitPullRequest, Undo2 } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import type { FileDiff } from "../../../api/types";
import { useAsync } from "../../../lib/useAsync";
import { useWaited } from "../../../lib/motion";
import { pullsApi } from "../api";
import { useWorkspace } from "../../../product/state/workspace";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { useUi } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { RestoreFilesDialog } from "./RestoreFilesDialog";
import { GitActionDialog } from "./GitActionDialog";
import { createPullRequestPlan } from "../../../product/git/plans";
import { useResolvedThreadPath } from "../../../product/composer/useResolvedThreadPath";
import { openInEditor } from "../files/openInEditor";
import { openChanged } from "./openChanged";
import { ChangedFiles } from "./ChangedFiles";
import { ChangesHeader, DIFF_SOURCES, WIDE, type ComparePair, type DiffSource } from "./ChangesHeader";
import { FileView } from "./FileView";
import { FindBar } from "./FindBar";
import { findHits, shownLimit, parsePatch, withoutWhitespaceChanges, type DiffLine } from "./diffModel";

const TEST_FILE = /(^|\/)(__tests__|tests?|e2e|spec)\/|\.(test|spec)\.[a-z]+$/i;
const FILE_PAGE = 50;
/** Where an empty scope sends the person: the scope nearest to it that is likelier to have something. */
const NEAREST: Record<DiffSource, DiffSource> = { worktree: "branch", turn: "worktree", thread: "worktree", branch: "worktree", commit: "branch" };
const SKELETON = [60, 90, 75, 85, 50, 80, 70];

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
      {change.conflict ? <span className="r"> · conflicted</span> : null}
    </p>
  );
}

/** Placeholder rows, drawn only if the read outlasts 400 ms. */
function Placeholders() {
  const slow = useWaited(true);
  return (
    <div role="status">
      <span className="sr-only">Loading changes</span>
      {slow ? SKELETON.map((width, at) => <div key={at} className="x2sk" aria-hidden style={{ width: `${width}%` }} />) : null}
    </div>
  );
}

const linesOf = (file: FileDiff, ignoreSpace: boolean): DiffLine[] => {
  const lines = parsePatch(file.patch);
  return ignoreSpace ? withoutWhitespaceChanges(lines) : lines;
};

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
  // The same read the header's Git menu uses: a checkout that can't answer Git questions hides what needs Git.
  const branches = useAsync(`git-branches:${threadId}`, (signal) => api.branches(threadId, signal));
  const gitAvailable = branches.status !== "error";
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
  const [pickedBase, setPickedBase] = useState<string | null>(null);
  const base = pickedBase ?? thread?.worktree?.base ?? "main";
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
  // a clean working tree after a commit, or Hide test files filtering a test-only change, still
  // leaves commits ahead of the base. The branch diff is read only where a pull could be made.
  const branchAhead = useAsync(`diff-branch-any:${threadId}:${base}:${canCreate}`, (signal) =>
    canCreate ? api.diff(threadId, signal, `branch&base=${encodeURIComponent(base)}`) : Promise.resolve([]),
  );
  const ignoreSpace = useUi((state) => state.diffIgnoreWhitespace);
  const hideTests = useUi((state) => state.diffHideTests);
  const collapsed = useUi((state) => state.diffFilesCollapsed);
  const reveal = useUi((state) => state.diffReveal);
  const resolved = useResolvedThreadPath(threadId, reveal ? { ...reveal, basis: "repository" } : null);
  const consume = useUi((state) => state.consumeDiffReveal);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [shownFully, setShownFully] = useState<ReadonlySet<string>>(new Set());
  const [pages, setPages] = useState(1);
  const [findOpen, setFindOpen] = useState(false);
  const [needle, setNeedle] = useState("");
  const [hitAt, setHitAt] = useState(0);
  const [width, setWidth] = useState(0);
  const root = useRef<HTMLDivElement>(null);

  // The panel's own width decides the header's rows, where the list sits and whether Split is on offer.
  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    setWidth(element.getBoundingClientRect().width);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => setWidth(entries[0]?.contentRect.width ?? 0));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const wide = width >= WIDE;

  const files = useMemo(() => {
    const every = diff.data ?? [];
    const kept = hideTests ? every.filter((file) => !TEST_FILE.test(file.path)) : every;
    // A conflict is listed first: it is the one thing that needs the person.
    return [...kept].sort((a, b) => Number(b.status === "conflicted") - Number(a.status === "conflicted"));
  }, [diff.data, hideTests]);
  const conflicts = files.filter((file) => file.status === "conflicted").length;
  const listed = useMemo(() => {
    const wanted = filter.trim().toLowerCase();
    return wanted ? files.filter((file) => file.path.toLowerCase().includes(wanted)) : files;
  }, [files, filter]);
  // With Collapse all files, nothing is open until a file is picked from the list.
  const selectedPath = pickedPath && files.some((file) => file.path === pickedPath) ? pickedPath : collapsed ? null : files[0]?.path ?? null;
  const selected = files.find((file) => file.path === selectedPath) ?? null;
  const selectedLines = useMemo(() => (selected ? linesOf(selected, ignoreSpace) : []), [selected, ignoreSpace]);

  const wanted = needle.trim().toLowerCase();
  const hits = useMemo(
    () => (findOpen && wanted ? findHits(files.map((file) => {
      // Lines cut off or collapsed are not drawn, so find does not count them until Show anyway draws them.
      const lines = linesOf(file, ignoreSpace);
      return { path: file.path, lines: lines.slice(0, shownLimit(lines.length, file.path, shownFully.has(file.path))) };
    }), wanted) : []),
    [findOpen, wanted, files, ignoreSpace, shownFully],
  );
  const hit = hits.length > 0 ? hits[Math.min(hitAt, hits.length - 1)] ?? null : null;
  // Turning Collapse all files on closes the file that was open.
  useEffect(() => { if (collapsed) setPickedPath(null); }, [collapsed]);
  // Stepping to a match in another file opens that file.
  // Only stepping to a match does this; picking another file in the list stays where the person put it.
  const hitKey = hit ? `${hit.path}:${hit.row}:${hit.start}` : "";
  const hitPath = hit?.path;
  useEffect(() => {
    if (hitPath) setPickedPath(hitPath);
  }, [hitKey, hitPath]);
  useEffect(() => setHitAt(0), [wanted]);
  useEffect(() => setPages(1), [source, filter]);

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
    // A revealed file opens in the list's view of it.
    if (files.some((file) => file.path === path)) {
      setPickedPath(path);
      consume(reveal.nonce);
    }
  }, [reveal, resolved.value, files, consume, threadId]);

  const pair: ComparePair | null = branches.status === "ready"
    ? {
        base,
        head: branches.data.find((branch) => branch.current)?.name ?? null,
        bases: [...new Set(branches.data.filter((branch) => !branch.current).map((branch) => branch.name))],
        enabled: source === "branch",
        onBase: setPickedBase,
      }
    : null;
  const scopeLabel = DIFF_SOURCES.find((entry) => entry.id === source)?.label ?? "";
  const loading = diff.status === "loading";
  const failed = diff.status === "error";
  const added = files.reduce((sum, file) => sum + file.additions, 0);
  const removed = files.reduce((sum, file) => sum + file.deletions, 0);
  // Nothing changed means nothing to open a pull request for.
  const offerPull = canCreate && ((diff.data?.length ?? 0) > 0 || (branchAhead.data?.length ?? 0) > 0);
  const foot = thread && thread.active_run === null && ((source === "turn" && latestRun) || offerPull);

  const bar = findOpen ? (
    <FindBar
      value={needle}
      onChange={setNeedle}
      at={Math.min(hitAt, Math.max(hits.length - 1, 0))}
      total={hits.length}
      onStep={(step) => setHitAt((at) => (hits.length === 0 ? 0 : (at + step + hits.length) % hits.length))}
      onClose={() => { setFindOpen(false); setNeedle(""); }}
    />
  ) : undefined;
  const view = selected ? (
    <FileView
      key={selected.path}
      file={selected}
      lines={selectedLines}
      threadId={threadId}
      origin={origin}
      needle={findOpen ? wanted : ""}
      current={hit && hit.path === selected.path ? { row: hit.row, start: hit.start } : null}
      full={shownFully.has(selected.path)}
      split={wide}
      onShowAll={() => setShownFully(new Set(shownFully).add(selected.path))}
      canRender={source === "worktree" || source === "thread"}
      bar={bar}
    />
  ) : null;
  // The list draws a page of files at a time, and always the one on show.
  const upTo = Math.max(pages * FILE_PAGE, listed.findIndex((file) => file.path === selectedPath) + 1);
  const list = (
    <ChangedFiles
      files={listed.slice(0, upTo)}
      more={Math.max(listed.length - upTo, 0)}
      selected={selectedPath}
      filter={filter}
      onFilter={setFilter}
      onPick={setPickedPath}
      onMore={() => setPages((now) => now + 1)}
      onCopy={(path) => void navigator.clipboard?.writeText(path).then(() => useUi.getState().pushToast("Path copied"))}
      onOpen={(path) => void openChanged(threadId, thread?.cwd ?? "", path, "editor")}
    />
  );

  return (
    <div
      ref={root}
      className="x2-pbody"
      data-wide={wide || undefined}
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f" && !event.altKey && !event.shiftKey) {
          event.preventDefault();
          setFindOpen(true);
        }
      }}
    >
      <JjChange threadId={threadId} version={version} />
      <ChangesHeader
        source={source}
        onSource={setSource}
        total={loading ? null : failed ? undefined : conflicts > 0 ? { conflicts } : { added, removed }}
        wide={wide}
        pair={pair}
        refreshing={diff.refreshing}
        onRefresh={reload}
      />
      {source === "commit" ? (
        <select className="diff-commit" aria-label="Commit" value={sha ?? ""} onChange={(event) => setPicked(event.target.value)}>
          {(commits.data ?? []).map((commit) => (
            <option key={commit.sha} value={commit.sha}>{commit.subject}</option>
          ))}
        </select>
      ) : null}
      {source === "turn" && turns.length > 0 ? (
        <select className="diff-commit" aria-label="Turn" value={latestRun ?? ""} onChange={(event) => setPickedRun(event.target.value)}>
          {turns.map((turn, at) => (
            <option key={turn.run_id} value={turn.run_id}>{`Turn ${at + 1} of ${turns.length}`}</option>
          ))}
        </select>
      ) : null}
      {loading ? (
        <Placeholders />
      ) : failed ? (
        <div className="x2banner err" role="alert" title={diff.error.message}>
          <span>Couldn’t read the changes.</span>
          <span className="sp" />
          <Button small tone="quiet" onClick={reload}>Try again</Button>
        </div>
      ) : files.length === 0 ? (
        <div className="x2empty">
          <DiffIcon size={22} aria-hidden />
          <h4>No changes in {scopeLabel}.</h4>
          {source === "worktree" ? <span>Everything is committed.</span> : null}
          <button type="button" className="linkish" onClick={() => setSource(NEAREST[source])}>
            Show {DIFF_SOURCES.find((entry) => entry.id === NEAREST[source])?.label} changes
          </button>
        </div>
      ) : (
        <>
          {conflicts > 0 ? (
            <div className="x2banner">
              <span>{conflicts === 1 ? "Merge conflict in 1 file." : `Merge conflict in ${conflicts} files.`} Resolve them in the editor, then refresh.</span>
              <span className="sp" />
              <Button small tone="quiet" onClick={() => void openInEditor(threadId, ".")}>Open in editor</Button>
            </div>
          ) : null}
          {wide ? (
            <div className="x2cols">
              <div className="x2main">{view ?? bar}</div>
              {list}
            </div>
          ) : (
            <>
              {list}
              <div className="x2main">{view ?? bar}</div>
            </>
          )}
        </>
      )}
      {foot ? (
        <div className="x2foot">
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
