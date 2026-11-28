import { CircleAlert, FileDiff as DiffIcon, GitPullRequest, Undo2 } from "lucide-react";
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
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { RestoreFilesDialog } from "./RestoreFilesDialog";
import { GitActionDialog } from "./GitActionDialog";
import { createPullRequestPlan } from "../../../product/git/plans";
import { useResolvedThreadPath } from "../../../product/composer/useResolvedThreadPath";
import { openInEditor } from "../files/openInEditor";
import { ChangedFiles, fileKey, type RepoFile } from "./ChangedFiles";
import { useHunkActions } from "./useHunkActions";
import { ChangesHeader, DIFF_SOURCES, NARROW, WIDE, type ComparePair, type DiffSource } from "./ChangesHeader";
import { FileView } from "./FileView";
import { FindBar } from "./FindBar";
import { commitItems, turnItems, type TurnTotals } from "./pickers";
import { chooseRepo, plain, RepoChooser, TrustRepo, usePickedRepo, useThreadRepos } from "../../repos";
import { findHits, shownLimit, parsePatch, withoutWhitespaceChanges, type DiffLine } from "./diffModel";

const TEST_FILE = /(^|\/)(__tests__|tests?|e2e|spec)\/|\.(test|spec)\.[a-z]+$/i;
/** The list draws this many rows; Show more files adds the rest. */
const FILE_CAP = 50;
/** Where an empty scope sends the person: the scope nearest to it that is likelier to have something. */
const NEAREST: Record<DiffSource, DiffSource> = { worktree: "branch", turn: "worktree", thread: "worktree", branch: "worktree", commit: "branch" };
/** The picker menus' widths in the design. */
const PICKER_WIDTH = { turn: 330, commit: 380 } as const;
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
  // A thread in several repositories groups its files; any other thread gets nothing here and keeps one flat list.
  const threadRepos = useThreadRepos(threadId, version);
  // The repository the branch and commit pickers act on: the primary until another is chosen. An ordinary thread has no choice.
  const choice = usePickedRepo(threadId, threadRepos.repos, threadRepos.reload);
  const repoId = choice.repoId;
  const repoSuffix = repoId === undefined ? "" : `:${repoId}`;
  const [source, setSource] = useState<DiffSource>("worktree");
  const checkpoints = useAsync(`checkpoints:${threadId}:${source === "turn" ? version : "off"}`, (signal) =>
    source === "turn" ? api.checkpoints(threadId, signal) : Promise.resolve([]),
  );
  const turns = checkpoints.data ?? [];
  const [pickedRun, setPickedRun] = useState<string | null>(null);
  const latestRun = (pickedRun && turns.some((turn) => turn.run_id === pickedRun) ? pickedRun : turns.at(-1)?.run_id) ?? null;
  // What each turn changed, for the turn picker's rows. A turn whose diff can't be read shows no totals. A thread in several
  // repositories sums the turn's diff of each trusted repository (the service answers one repository per read); an untrusted
  // repository adds nothing and a repository whose read fails is left out, and the row says how many were not counted.
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id ?? "");
  const totalsRepos = threadRepos.repos;
  const totals = useAsync(`turn-totals:${threadId}:${version}:${turns.map((turn) => turn.run_id).join(",")}:${totalsRepos ? totalsRepos.map((repo) => `${repo.repo_id}${repo.trusted ? "" : "!"}`).join(",") : ""}`, (signal) => {
    if (source !== "turn" || turns.length === 0) return Promise.resolve([]);
    const sumOf = (files: FileDiff[]) => ({ added: files.reduce((sum, file) => sum + file.additions, 0), removed: files.reduce((sum, file) => sum + file.deletions, 0), files: files.length });
    return Promise.all(turns.map(async (turn): Promise<TurnTotals | null> => {
      const scope = `turn&run=${encodeURIComponent(turn.run_id)}`;
      if (!totalsRepos || totalsRepos.length === 0) return api.diff(threadId, signal, scope).then(sumOf, () => null);
      const reads = await Promise.all(totalsRepos.filter((repo) => repo.trusted).map((repo) =>
        api.diff(threadId, signal, scope, repo.repo_id === projectId ? undefined : repo.repo_id).then(sumOf, () => null)));
      const counted = reads.filter((one): one is NonNullable<typeof one> => one !== null);
      if (counted.length === 0) return null;
      return { added: counted.reduce((sum, one) => sum + one.added, 0), removed: counted.reduce((sum, one) => sum + one.removed, 0), files: counted.reduce((sum, one) => sum + one.files, 0), skipped: totalsRepos.length - counted.length };
    }));
  });
  // Where the turn or commit picker is open, under the header.
  const [pickerAt, setPickerAt] = useState<{ x: number; y: number } | null>(null);
  const thread = useWorkspace((state) => state.threads[threadId]);
  const linkedPulls = useAsync(`diff-linked:${threadId}`, (signal) => pullsApi.linked(threadId, signal));
  // The same read the header's Git menu uses: a checkout that can't answer Git questions hides what needs Git.
  const branches = useAsync(`git-branches:${threadId}`, (signal) => api.branches(threadId, signal));
  // The chosen repository's own branches, for the base and commit pickers; the primary's read above is the one used when none is chosen.
  const chosenBranches = useAsync(`git-branches:${threadId}${repoSuffix}`, (signal) => repoId === undefined ? Promise.resolve(null) : api.branches(threadId, signal, repoId));
  const pairBranches = repoId === undefined ? (branches.status === "ready" ? branches.data : null) : chosenBranches.status === "ready" ? chosenBranches.data : null;
  const gitAvailable = branches.status !== "error";
  const canCreate = gitAvailable && linkedPulls.status === "ready" && linkedPulls.data.length === 0;
  const [restoring, setRestoring] = useState(false);
  const [creating, setCreating] = useState(false);
  const commits = useAsync(`commits:${threadId}:${source}${repoSuffix}`, (signal) =>
    source === "commit" ? (repoId === undefined ? api.commits(threadId, signal) : api.commits(threadId, signal, repoId)) : Promise.resolve([]),
  );
  const [picked, setPicked] = useState<string | null>(null);
  const commitReveal = useUi((state) => state.commitReveal);
  useEffect(() => {
    if (!commitReveal) return;
    setSource("commit");
    setPicked(commitReveal.sha);
    // A commit named by a link is the primary repository's.
    chooseRepo(threadId, null);
  }, [commitReveal, threadId]);
  const sha = picked ?? commits.data?.[0]?.sha ?? null;
  const [pickedBase, setPickedBase] = useState<string | null>(null);
  // A base or commit belongs to the repository it was picked in.
  const seenRepo = useRef(repoId);
  useEffect(() => {
    if (seenRepo.current === repoId) return;
    seenRepo.current = repoId;
    // A commit a link just revealed is the primary's own and stays.
    if (commitReveal && picked === commitReveal.sha) return;
    setPickedBase(null);
    setPicked(null);
  }, [repoId]); // eslint-disable-line react-hooks/exhaustive-deps
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
  // The project's own folder is the primary repository and its id is the project's: its reads and writes leave `repo_id` out.
  // The service answers one repository's files per read (`?repo_id=`), and a file carries no repository of its own. So a thread in
  // several repositories reads once per repository and marks each file with the repository it came from; any other thread reads once.
  const repoList = threadRepos.repos;
  const repoKey = repoList ? repoList.map((repo) => repo.repo_id).join(",") : "";
  const diff = useAsync(`diff:${threadId}:${scope ?? "none"}:${repoKey}${source === "commit" || source === "branch" ? repoSuffix || ":primary" : ""}`, async (signal): Promise<RepoFile[]> => {
    if (!scope) return [];
    if (!repoList || repoList.length === 0) return api.diff(threadId, signal, scope);
    // An untrusted repository reports no changes and its reads are refused (`409 repo_untrusted`): it is not read. A commit or a
    // branch comparison is one repository's, so those scopes read only the repository the pickers are on.
    const only = (source === "commit" || source === "branch") && choice.selected ? choice.selected.repo_id : null;
    const reads = await Promise.allSettled(repoList.filter((repo) => repo.trusted && (only === null || repo.repo_id === only)).map(async (repo) => {
      const files = await api.diff(threadId, signal, scope, repo.repo_id === projectId ? undefined : repo.repo_id);
      return files.map((file): RepoFile => ({ ...file, repo_id: repo.repo_id }));
    }));
    // A repository that can't be read (untrusted, say) lists nothing; the thread's Changes fail only when none could be read.
    const failed = reads.find((read): read is PromiseRejectedResult => read.status === "rejected");
    if (failed && reads.every((read) => read.status === "rejected")) throw failed.reason;
    return reads.flatMap((read) => (read.status === "fulfilled" ? read.value : []));
  });
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
  // The file on show is named by `fileKey` (repository and path), so two README.md files in different repositories are two files.
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [shownFully, setShownFully] = useState<ReadonlySet<string>>(new Set());
  const [allFiles, setAllFiles] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [needle, setNeedle] = useState("");
  const [hitAt, setHitAt] = useState(0);
  const [width, setWidth] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const placePicker = (kind: "turn" | "commit") => {
    const head = root.current?.querySelector(".ch2")?.getBoundingClientRect();
    if (head) setPickerAt({ x: Math.max(8, head.right - PICKER_WIDTH[kind] - 8), y: head.bottom + 4 });
  };
  const openPicker = () => {
    if (source === "turn" || source === "commit") placePicker(source);
  };
  // Choosing Turn or Commit in the scope menu changes the scope and opens the list to pick which one.
  const pickSource = (next: DiffSource) => {
    setSource(next);
    setPickerAt(null);
    if (next === "turn" || next === "commit") placePicker(next);
  };

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
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd ?? "");
  const hunkActions = useHunkActions(threadId, cwd);
  // The trust prompt a refused hunk or line write offers; the write is never repeated by it.
  const [trusting, setTrusting] = useState<string | null>(null);
  const pushToast = useUi((state) => state.pushToast);

  const files = useMemo(() => {
    const every: RepoFile[] = diff.data ?? [];
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
  const selectedPath = pickedPath && files.some((file) => fileKey(file) === pickedPath) ? pickedPath : collapsed ? null : files[0] ? fileKey(files[0]) : null;
  const selected = files.find((file) => fileKey(file) === selectedPath) ?? null;
  const selectedLines = useMemo(() => (selected ? linesOf(selected, ignoreSpace) : []), [selected, ignoreSpace]);

  // In a multi-repository thread the file header names the repository, so the diff always says whose README.md it shows.
  const repoNameOf = (file: RepoFile): string | undefined => {
    const repo = file.repo_id === undefined ? undefined : threadRepos.repos?.find((one) => one.repo_id === file.repo_id);
    return repo ? plain(repo.name) : undefined;
  };

  const wanted = needle.trim().toLowerCase();
  const hits = useMemo(
    () => (findOpen && wanted ? findHits(files.map((file) => {
      // Lines cut off or collapsed are not drawn, so find does not count them until Show anyway draws them.
      const lines = linesOf(file, ignoreSpace);
      return { path: fileKey(file), lines: lines.slice(0, shownLimit(lines.length, file.path, shownFully.has(fileKey(file)))) };
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
  useEffect(() => setAllFiles(false), [source, filter]);

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
    const found = files.find((file) => file.path === path);
    if (found) {
      setPickedPath(fileKey(found));
      consume(reveal.nonce);
    }
  }, [reveal, resolved.value, files, consume, threadId]);

  const pair: ComparePair | null = pairBranches
    ? {
        base,
        head: pairBranches.find((branch) => branch.current)?.name ?? null,
        bases: [...new Set(pairBranches.filter((branch) => !branch.current).map((branch) => branch.name))],
        enabled: source === "branch",
        onBase: setPickedBase,
      }
    : null;
  const commitList = commits.data ?? [];
  const turnAt = latestRun ? turns.findIndex((turn) => turn.run_id === latestRun) : -1;
  // Turn and Commit compare something definite, so their pair names it and opens the picker.
  const pickerPair: ComparePair | null =
    source === "turn" && turnAt >= 0
      ? { base: turnAt > 0 ? `turn ${turnAt}` : "start", head: `turn ${turnAt + 1}`, bases: [], enabled: true, onBase: () => undefined, onOpen: openPicker }
      : source === "commit" && sha
        ? { base: `${sha.slice(0, 7)}^`, head: sha.slice(0, 7), bases: [], enabled: true, onBase: () => undefined, onOpen: openPicker }
        : null;
  const pickerItems = source === "turn"
    ? turnItems(turns, totals.data ?? [], latestRun, setPickedRun)
    : source === "commit" ? commitItems(commitList, sha, setPicked) : [];
  const scopeLabel = DIFF_SOURCES.find((entry) => entry.id === source)?.label ?? "";
  const loading = diff.status === "loading";
  const failed = diff.status === "error";
  const added = files.reduce((sum, file) => sum + file.additions, 0);
  const removed = files.reduce((sum, file) => sum + file.deletions, 0);
  // Nothing changed means nothing to open a pull request for.
  const offerPull = canCreate && ((diff.data?.length ?? 0) > 0 || (branchAhead.data?.length ?? 0) > 0);
  const foot = thread && thread.active_run === null && ((source === "turn" && latestRun) || offerPull);

  // A file of a multi-repository thread's primary repository opens as a tab that names it, and still reads and writes as
  // the thread's own; a file of another repository opens as that repository's tab, which sends its repo_id. Any other
  // thread's files are the thread's own, with no repository on them.
  const primaryRepoId = choice.multi && selected !== null && selected.repo_id !== undefined && selected.repo_id === choice.primary?.repo_id ? selected.repo_id : undefined;
  const changedRepoId = selected !== null && selected.repo_id !== undefined && selected.repo_id !== (choice.primary?.repo_id ?? projectId) ? selected.repo_id : undefined;
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
      key={fileKey(selected)}
      file={selected}
      lines={selectedLines}
      threadId={threadId}
      origin={origin}
      needle={findOpen ? wanted : ""}
      current={hit && hit.path === fileKey(selected) ? { row: hit.row, start: hit.start } : null}
      full={shownFully.has(fileKey(selected))}
      repoName={repoNameOf(selected)}
      repoId={changedRepoId}
      primaryRepoId={primaryRepoId}
      // The primary repository is the thread's own checkout, which carries no repository anywhere else either: stamped with
      // its id, its comments would never match the ones a file tab makes on that same file, and a save from that tab
      // would leave them unmoved and unmarked. Every other repository's comments carry its id.
      fileRepoId={changedRepoId}
      split={wide}
      onShowAll={() => setShownFully(new Set(shownFully).add(fileKey(selected)))}
      canRender={source === "worktree" || source === "thread"}
      bar={bar}
      edits={source === "worktree" && !ignoreSpace ? { revert: hunkActions.revert, saveLine: hunkActions.saveLine } : undefined}
      narrow={width > 0 && width < NARROW}
    />
  ) : null;
  // The list draws its first 50 rows, and always the one on show (a picked file or a find match opens past the cap).
  const upTo = allFiles ? listed.length : Math.max(FILE_CAP, listed.findIndex((file) => fileKey(file) === selectedPath) + 1);
  const list = (
    <ChangedFiles
      files={listed.slice(0, upTo)}
      more={Math.max(listed.length - upTo, 0)}
      selected={selectedPath}
      filter={filter}
      onFilter={setFilter}
      onPick={setPickedPath}
      onMore={() => setAllFiles(true)}
      repos={threadRepos.repos}
      threadId={threadId}
      onRepoChanged={threadRepos.reload}
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
      {choice.multi && (source === "commit" || source === "branch") ? (
        <div className="repos-scope"><span className="field-hint">Repository</span><RepoChooser choice={choice} /></div>
      ) : null}
      <ChangesHeader
        source={source}
        onSource={pickSource}
        total={loading ? null : failed ? undefined : conflicts > 0 ? { conflicts } : { added, removed }}
        wide={wide}
        pair={pickerPair ?? pair}
        refreshing={diff.refreshing}
        onRefresh={reload}
      />
      {hunkActions.notice ? (
        <div className="hunk-banner" role="alert">
          <CircleAlert size={14} aria-hidden />
          <span><b>{hunkActions.notice.title}</b> {hunkActions.notice.detail}</span>
          {hunkActions.notice.untrusted && (hunkActions.notice.untrusted.repoId !== null || choice.multi) ? (
            <Button small onClick={() => setTrusting(hunkActions.notice?.untrusted?.repoId ?? projectId)}>Trust repository…</Button>
          ) : null}
          <Button small onClick={() => { hunkActions.clearNotice(); reload(); }}>{hunkActions.notice.action}</Button>
        </div>
      ) : null}
      {hunkActions.reverted ? (
        <div className="hunk-toast" role="status">
          <span>Hunk reverted in {hunkActions.reverted.path}.</span>
          <span className="ta">
            <Button small tone="quiet" onClick={() => void hunkActions.undo()}>Undo · {hunkActions.reverted.seconds} s</Button>
            <Button small tone="quiet" onClick={hunkActions.dismissReverted}>Dismiss</Button>
          </span>
        </div>
      ) : null}
      {trusting ? (
        <TrustRepo projectId={projectId} repoId={trusting} name={choice.repos.find((one) => one.repo_id === trusting)?.name ?? "This repository"}
          onClose={() => setTrusting(null)}
          onDone={() => { hunkActions.clearNotice(); threadRepos.reload(); reload(); pushToast("Trusted. Try the change again."); }} />
      ) : null}
      <Presence>
        {pickerAt && pickerItems.length > 0 && (source === "turn" || source === "commit") ? (
          <Menu
            x={pickerAt.x}
            y={pickerAt.y}
            label={source === "turn" ? "Turn" : "Commit"}
            heading={source === "turn" ? "Turns in this thread" : `Commits on ${pairBranches?.find((branch) => branch.current)?.name ?? "this branch"}`}
            minWidth={PICKER_WIDTH[source]}
            items={pickerItems}
            onClose={() => setPickerAt(null)}
          />
        ) : null}
      </Presence>
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
