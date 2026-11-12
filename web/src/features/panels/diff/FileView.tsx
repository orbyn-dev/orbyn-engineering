import { SquarePen } from "lucide-react";
import type { ReactNode } from "react";
import type { FileDiff } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { ErrorState } from "../../../product/ui/states";
import { Button, IconButton } from "../../../ui/controls";
import { ChangeLetter } from "../../workspace";
import { api, readThreadFile } from "../api";
import { openInEditor } from "../files/openInEditor";
import { FilePreview, previewKind } from "../files/FilePreview";
import { Patch } from "./DiffPatch";
import { fileName } from "./ChangedFiles";
import { changeOf, isBinaryPatch, isLockFile, renameOf, type DiffLine } from "./diffModel";

const count = (value: number) => value.toLocaleString("en-US");

/** The checkout-relative path of a file the diff names from the repository's root, or null when the thread's folder doesn't hold it. */
async function checkoutPath(threadId: string, cwd: string, path: string, signal?: AbortSignal) {
  const resolved = await api.resolveThreadPath(threadId, { path, basis: "repository", cwd }, signal);
  return resolved.checkout_path;
}

/** Opens the file in the person's editor, or in a file tab when the editor is not what is wanted (a binary file's Open). */
async function open(threadId: string, cwd: string, path: string, how: "editor" | "tab") {
  try {
    const inside = await checkoutPath(threadId, cwd, path);
    if (inside === null) {
      useUi.getState().pushToast("Couldn't open it. That file is outside this thread's folder.");
      return;
    }
    if (how === "tab") useUi.getState().openFileTab({ kind: "thread", path: inside });
    else await openInEditor(threadId, inside);
  } catch (failure) {
    useUi.getState().pushToast(`Couldn't open it. ${toFailure(failure).message}`);
  }
}

/** A Markdown file as the reader will see it, read from the checkout. */
function Rendered({ threadId, cwd, path }: { threadId: string; cwd: string; path: string }) {
  const read = useAsync(`diff-rendered:${JSON.stringify([threadId, cwd, path])}`, async (signal) => {
    const inside = await checkoutPath(threadId, cwd, path, signal);
    return inside === null ? null : readThreadFile(threadId, inside, signal);
  });
  if (read.status === "error") return <ErrorState compact title="Couldn't read this file" failure={read.error} onRetry={read.reload} />;
  if (read.status === "loading") return <div className="md2" role="status"><span className="sr-only">Loading file</span></div>;
  if (!read.data || read.data.binary) return <p className="x2note t3">This file can't be shown rendered.</p>;
  return (
    <div className="md2">
      <FilePreview kind="markdown" text={read.data.content} />
    </div>
  );
}

/** Rendered or Source for a Markdown file. The choice is kept for the type. */
function RenderToggle({ rendered }: { rendered: boolean }) {
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <span className="segx x2seg" role="radiogroup" aria-label="View" title={rendered ? "Switch to Source to comment on lines" : undefined}>
      {([[true, "Rendered"], [false, "Source"]] as const).map(([value, label]) => (
        <button key={label} type="button" role="radio" aria-checked={rendered === value} className={rendered === value ? "on" : ""}
          onClick={() => setPrefs({ diffMarkdownRendered: value })}>
          {label}
        </button>
      ))}
    </span>
  );
}

/** One file of the change: its header, then its lines, or the one line that says why there are none. */
export function FileView({ file, lines, threadId, origin, needle, current, full, split, onShowAll, canRender, bar }: {
  file: FileDiff;
  lines: readonly DiffLine[];
  threadId: string;
  origin: string;
  needle: string;
  current: { row: number; start: number } | null;
  full: boolean;
  split: boolean;
  onShowAll(): void;
  /** The checkout holds this file as the change leaves it, so it can be drawn rendered. */
  canRender: boolean;
  /** The find bar, when it is open: it sits between the file's header and its lines. */
  bar?: ReactNode;
}) {
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd ?? "");
  const renderedPref = useUi((state) => state.diffMarkdownRendered);
  const binary = isBinaryPatch(file.patch);
  const markdown = previewKind(file.path) === "markdown" && canRender && file.status !== "deleted" && !binary;
  const rendered = markdown && renderedPref;
  const conflict = file.status === "conflicted";
  return (
    <div className="x2file" data-diff-file={file.path}>
      <div className="fh2">
        <ChangeLetter change={changeOf(file)} />
        <span className="nm" title={fileName(file)}>{file.path}</span>
        {markdown ? <RenderToggle rendered={rendered} /> : null}
        {conflict ? (
          <span className="num">conflict</span>
        ) : binary ? null : (
          <span className="num">
            +{count(file.additions)} <span className="r">−{count(file.deletions)}</span>
          </span>
        )}
        <IconButton label={binary ? "Open" : "Open in editor"} onClick={() => void open(threadId, cwd, file.path, binary ? "tab" : "editor")}>
          <SquarePen size={14} aria-hidden />
        </IconButton>
      </div>
      {bar}
      {rendered ? (
        <Rendered threadId={threadId} cwd={cwd} path={file.path} />
      ) : binary ? (
        <div className="x2banner">
          <span>Binary file changed.</span>
          <span className="sp" />
          <Button small tone="quiet" onClick={() => void open(threadId, cwd, file.path, "tab")}>Open</Button>
        </div>
      ) : lines.length === 0 ? (
        <p className="x2note t3">
          {file.status === "renamed" && renameOf(file.patch) ? "Renamed, no changes to the lines" : "No text changes to show."}
        </p>
      ) : (
        <Patch lines={lines} path={file.path} threadId={threadId} origin={origin} needle={needle} current={current}
          lock={isLockFile(file.path)} full={full} split={split} onShowAll={onShowAll} />
      )}
    </div>
  );
}
