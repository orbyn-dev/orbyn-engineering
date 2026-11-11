import { ChevronRight, Folder, Lock } from "lucide-react";
import { useEffect } from "react";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { formatBytes } from "../../../lib/format";
import { useAsync } from "../../../lib/useAsync";
import { useResolvedThreadPath } from "../../../product/composer/useResolvedThreadPath";
import { parseFileTab, useUi, type FileTabId, type FileTarget } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { canStartThreadIn, readExplorerFile, readOnlyWhy, startThreadIn, threadPathOf, useExplorer, useFileTabNames } from "../../workspace";
import { Viewer } from "./FileViewer";
import { OutsideFileViewer, fullPathOf, isFullPath } from "./OutsideFileViewer";

type ExplorerTarget = Extract<FileTarget, { kind: "explorer" }>;

/**
 * A file of the thread's own checkout, named by a link in a message or by the thread's worktree in the
 * left bar. The path may be one inside the checkout or a full path outside it, which only reads.
 */
function ThreadFile({ threadId, path }: { threadId: string; path: string }) {
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const reveal = useUi((state) => state.fileReveal);
  const consume = useUi((state) => state.consumeFileReveal);
  // A link that named a line says it once; the line stays for this open.
  useEffect(() => {
    if (reveal && reveal.threadId === threadId && reveal.path === path) consume(reveal.nonce);
  }, [reveal, threadId, path, consume]);
  const line = reveal && reveal.threadId === threadId && reveal.path === path ? reveal.line : null;
  const resolved = useResolvedThreadPath(threadId, cwd ? { threadId, cwd, path, basis: "thread", nonce: 0 } : null);
  const full = fullPathOf(path);
  if (resolved.value?.checkout_path) return <Viewer threadId={threadId} path={resolved.value.checkout_path} line={line} />;
  if (resolved.value && isFullPath(full)) return <OutsideFileViewer threadId={threadId} path={full} />;
  if (resolved.value || resolved.error) return <Empty title={resolved.error?.message ?? "That path is outside this checkout."} />;
  return <Loading label="Loading file" rows={6} />;
}

/** The line numbers and text of a file read as it is, one row per line. */
function Code({ text }: { text: string }) {
  const lines = text.split("\n");
  if (lines.at(-1) === "") lines.pop();
  return (
    <div className="x2code" role="region" aria-label="File contents" tabIndex={0}>
      {lines.map((line, at) => (
        <div key={at} className="l">
          <i>{at + 1}</i>
          <code>{line}</code>
        </div>
      ))}
    </div>
  );
}

/** A file of any worktree, read through the explorer and never written. */
function ReadOnlyFile({ threadId, target, worktree }: { threadId: string; target: ExplorerTarget; worktree: ExplorerWorktree | undefined }) {
  const read = useAsync(`explorer-file:${JSON.stringify([target.projectId, target.worktreeId, target.path])}`, (signal) =>
    readExplorerFile(target.projectId, target.worktreeId, target.path, signal),
  );
  const file = read.status === "ready" ? read.data : null;
  const why = worktree ? readOnlyWhy(worktree, threadId) : file?.read_only ? "other_session" : null;
  const names = useFileTabNames(target.projectId, target.worktreeId, threadId);
  const threads = useWorkspace((state) => state.threads);
  const folders = target.path.split("/").slice(0, -1);
  const name = target.path.split("/").at(-1) ?? target.path;
  return (
    <div className="explorer-file" data-read-only={why ? "true" : undefined}>
      {why ? (
        <div className="x2banner" role="status">
          <Lock size={14} aria-hidden />
          <span>{why === "not_this_thread" ? "Read-only · not this thread’s worktree" : "Read-only"}</span>
          <span className="sp" />
          {why === "not_this_thread" && worktree && canStartThreadIn(worktree, Object.values(threads)) ? (
            <Button small tone="quiet" onClick={() => startThreadIn(target.projectId, worktree)}>
              Start a thread here
            </Button>
          ) : null}
        </div>
      ) : null}
      <div className="x2crumb" title={names.project && names.worktree ? `${names.project} · ${names.worktree}` : undefined}>
        <Folder size={14} aria-hidden />
        {folders.length > 0 ? folders.map((part, at) => (
          <span key={at} className="x2crumb-part">
            {part}
            <ChevronRight size={12} aria-hidden />
          </span>
        )) : null}
        <b>{name}</b>
      </div>
      {read.status === "loading" ? (
        <Loading label="Loading file" rows={6} />
      ) : read.status === "error" && read.error.status === 404 ? (
        <Empty title="This file isn't in this worktree." />
      ) : read.status === "error" ? (
        <ErrorState compact title="Couldn't read this file" failure={read.error} onRetry={read.reload} />
      ) : !file ? null : file.kind === "too_large" ? (
        <Empty title={`This file is too large to show here (${formatBytes(file.size)}).`} />
      ) : file.kind === "binary" ? (
        <Empty title={`This file isn't text (${formatBytes(file.size)}).`} />
      ) : (
        <Code text={file.content} />
      )}
    </div>
  );
}

/** A file of the explorer's tree. In the worktree this thread runs in it is the thread's own file, with everything that has. */
function ExplorerFile({ threadId, target }: { threadId: string; target: ExplorerTarget }) {
  const kept = useExplorer((state) => state.worktrees[target.projectId]);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  useEffect(() => {
    if (!kept) void useExplorer.getState().loadWorktrees(target.projectId, { thread: threadId });
  }, [kept, target.projectId, threadId]);
  const worktree = kept?.data?.worktrees.find((one) => one.id === target.worktreeId);
  if (!kept || (kept.loading && !kept.data)) return <Loading label="Loading file" rows={6} />;
  // The thread's folder may be a subfolder of the worktree (the service roots a worktree at the Git root): the thread names
  // a file relative to its own folder, and reads only what is inside it.
  const own = worktree && cwd ? threadPathOf(worktree.path, cwd, target.path) : null;
  if (own !== null) return <ThreadFile threadId={threadId} path={own} />;
  return <ReadOnlyFile threadId={threadId} target={target} worktree={worktree} />;
}

/** What a file tab shows. */
export function FileTab({ id, threadId }: { id: FileTabId; threadId: string }) {
  const target = parseFileTab(id);
  if (!target) return null;
  return target.kind === "thread" ? <ThreadFile threadId={threadId} path={target.path} /> : <ExplorerFile threadId={threadId} target={target} />;
}
