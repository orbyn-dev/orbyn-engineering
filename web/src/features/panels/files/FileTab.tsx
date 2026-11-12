import { Lock } from "lucide-react";
import { useEffect, useState } from "react";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import { formatBytes } from "../../../lib/format";
import { useAsync } from "../../../lib/useAsync";
import { useResolvedThreadPath } from "../../../product/composer/useResolvedThreadPath";
import { parseFileTab, useUi, type FileTabId, type FileTarget } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { canStartThreadIn, readExplorerFile, startThreadIn, threadPathOf, useExplorer, useFileTabNames } from "../../workspace";
import { EditorLoading } from "../editor/EditorShell";
import { EditorSurface } from "../editor/EditorSurface";
import { useEditorSessions } from "../editor/editorSessions";
import { Viewer } from "./FileViewer";
import { OutsideFileViewer, fullPathOf, isFullPath } from "./OutsideFileViewer";

type ExplorerTarget = Extract<FileTarget, { kind: "explorer" }>;

/**
 * A file of the thread's own checkout, named by a link in a message or by the thread's worktree in the
 * left bar. The path may be one inside the checkout or a full path outside it, which only reads.
 */
function ThreadFile({ threadId, tabId, path }: { threadId: string; tabId: string; path: string }) {
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const reveal = useUi((state) => state.fileReveal);
  const consume = useUi((state) => state.consumeFileReveal);
  // A link that named a line says it once; the line stays for this open.
  const [line, setLine] = useState<number | null>(null);
  useEffect(() => {
    if (!reveal || reveal.threadId !== threadId || reveal.path !== path) return;
    setLine(reveal.line);
    consume(reveal.nonce);
  }, [reveal, threadId, path, consume]);
  const resolved = useResolvedThreadPath(threadId, cwd ? { threadId, cwd, path, basis: "thread", nonce: 0 } : null);
  const full = fullPathOf(path);
  if (resolved.value?.checkout_path) return <Viewer threadId={threadId} tabId={tabId} path={resolved.value.checkout_path} line={line} />;
  if (resolved.value && isFullPath(full)) return <OutsideFileViewer threadId={threadId} path={full} />;
  if (resolved.value || resolved.error) return <Empty title={resolved.error?.message ?? "That path is outside this checkout."} />;
  return <Loading label="Loading file" rows={6} />;
}

/** A file of any worktree, read through the explorer and never written. */
function ReadOnlyFile({ threadId, tabId, target, worktree }: { threadId: string | null; tabId: string; target: ExplorerTarget; worktree: ExplorerWorktree | undefined }) {
  const read = useAsync(`explorer-file:${JSON.stringify([target.projectId, target.worktreeId, target.path])}`, (signal) =>
    readExplorerFile(target.projectId, target.worktreeId, target.path, signal),
  );
  const file = read.status === "ready" ? read.data : null;
  // Only the drawn banner: this is not the worktree the thread runs in.
  const locked = worktree?.read_only_reason === "locked";
  const notMine = !!worktree && !worktree.is_subfolder && !locked && (threadId === null || !worktree.threads.some((one) => one.id === threadId));
  const names = useFileTabNames(target.projectId, target.worktreeId, threadId);
  const threads = useWorkspace((state) => state.threads);
  // The header says Read-only beside the worktree's name.
  const setLock = useEditorSessions((state) => state.lock);
  const readOnly = notMine || locked;
  useEffect(() => {
    setLock(tabId, readOnly);
    return () => setLock(tabId, false);
  }, [tabId, readOnly, setLock]);
  const banner = (
    <>
      {locked ? (
        <div className="x2banner" role="status">
          <Lock size={14} aria-hidden />
          <span>This worktree is locked. Files open read-only.</span>
        </div>
      ) : null}
      {notMine ? (
        <div className="ed-banner" role="status">
          <Lock size={14} aria-hidden />
          <div>
            {threadId === null ? (
              "Read-only · no thread is open"
            ) : (
              <>
                <b>Read-only · not this thread’s worktree.</b>
                {names.worktree ? ` This file is in ${names.worktree}.` : ""}
              </>
            )}
          </div>
          {worktree && canStartThreadIn(worktree, Object.values(threads)) ? (
            <div className="ed-banner-end">
              <Button small onClick={() => startThreadIn(target.projectId, worktree)}>
                Start a thread here
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
  return (
    <div className="explorer-file" data-read-only={notMine || locked ? "true" : undefined}>
      {/* A text file carries the banner inside the editor, under the folders; any other state shows it first. */}
      {file && file.kind !== "binary" && file.kind !== "too_large" ? null : banner}
      {read.status === "loading" ? (
        <EditorLoading path={target.path} />
      ) : read.status === "error" && read.error.status === 404 ? (
        <Empty title="This file isn't in this worktree." />
      ) : read.status === "error" ? (
        <ErrorState compact title="Couldn't read this file" failure={read.error} onRetry={read.reload} />
      ) : !file ? null : file.kind === "too_large" ? (
        <Empty title={`This file is too large to show here (${formatBytes(file.size)}).`} />
      ) : file.kind === "binary" ? (
        <Empty title={`This file isn't text (${formatBytes(file.size)}).`} />
      ) : (
        <EditorSurface path={target.path} text={file.content} standing="read-only" banner={banner} />
      )}
    </div>
  );
}

/** A file of the explorer's tree. In the worktree this thread runs in it is the thread's own file, with everything that has. */
function ExplorerFile({ threadId, tabId, target }: { threadId: string | null; tabId: string; target: ExplorerTarget }) {
  const kept = useExplorer((state) => state.worktrees[target.projectId]);
  const cwd = useWorkspace((state) => (threadId ? state.threads[threadId]?.cwd : undefined));
  useEffect(() => {
    if (!kept) void useExplorer.getState().loadWorktrees(target.projectId, { thread: threadId });
  }, [kept, target.projectId, threadId]);
  const worktree = kept?.data?.worktrees.find((one) => one.id === target.worktreeId);
  if (!kept || (kept.loading && !kept.data)) return <Loading label="Loading file" rows={6} />;
  // The thread's folder may be a subfolder of the worktree (the service roots a worktree at the Git root): the thread names
  // a file relative to its own folder, and reads only what is inside it.
  const own = threadId && worktree && cwd ? threadPathOf(worktree.path, cwd, target.path) : null;
  if (threadId && own !== null) return <ThreadFile threadId={threadId} tabId={tabId} path={own} />;
  return <ReadOnlyFile threadId={threadId} tabId={tabId} target={target} worktree={worktree} />;
}

/** A worktree whose folder is gone: nothing to browse, and a way to read the list again. */
function MissingWorktree({ target, threadId }: { target: Extract<FileTarget, { kind: "worktree" }>; threadId: string | null }) {
  return (
    <div className="explorer-file">
      <div className="x2banner" role="status">
        <Lock size={14} aria-hidden />
        <span>This worktree is missing from disk.</span>
      </div>
      <Empty title="Nothing to browse">
        <p className="t3">Its folder was moved or deleted. Restore the folder and refresh, or remove the worktree from the list.</p>
        <Button small tone="quiet" onClick={() => void useExplorer.getState().loadWorktrees(target.projectId, { thread: threadId, refresh: true })}>
          Refresh
        </Button>
      </Empty>
    </div>
  );
}

/** What a file tab shows. With no thread open only the explorer's own tabs can show: a thread file has no thread to name it. */
export function FileTab({ id, threadId }: { id: FileTabId; threadId: string | null }) {
  const target = parseFileTab(id);
  if (!target) return null;
  if (target.kind === "worktree") return <MissingWorktree target={target} threadId={threadId} />;
  if (target.kind === "thread") return threadId === null ? null : <ThreadFile threadId={threadId} tabId={id} path={target.path} />;
  return <ExplorerFile threadId={threadId} tabId={id} target={target} />;
}
