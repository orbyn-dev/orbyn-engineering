import { AppWindow, Copy, Download, ExternalLink, Paperclip } from "lucide-react";
import { useEffect, useState } from "react";
import { openThreadFileInBrowser, readThreadFile, saveThreadFile } from "../api";
import type { FileContent } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { chipId, fileExcerpt, languageForPath } from "../../../product/composer/composerContext";
import { Button, IconButton } from "../../../ui/controls";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { DocumentViewer, officeKind } from "./DocumentViewer";
import { FileBytes, FileMedia } from "./FileMedia";
import { mediaKind, rawUrl } from "../../../product/media/media";
import { FilePreview, previewKind } from "./FilePreview";
import { ZipViewer, isZipPath } from "./ZipViewer";
import { openInEditor } from "./openInEditor";
import { useWorkspace } from "../../../product/state/workspace";
import { FileComments } from "./FileComments";
import { formatBytes } from "../../../lib/format";
import { BinaryFile } from "../editor/BinaryFile";
import { EditorLoading } from "../editor/EditorShell";
import { FileEditor } from "../editor/FileEditor";
import { useEditorSessions } from "../editor/editorSessions";


/** Reads a file by what it holds, with the source a click away and edits saved safely. */
function FileBody({
  threadId,
  path,
  file,
  line,
  cwd,
  onSaved,
}: {
  threadId: string;
  path: string;
  file: FileContent;
  line: number | null;
  cwd: string;
  onSaved(): void;
}) {
  const kind = previewKind(path);
  const [mode, setMode] = useState<"preview" | "source" | "edit">(kind && !line ? "preview" : "source");
  const [text, setText] = useState(file.content);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await saveThreadFile(threadId, { path, content: text, base_revision: file.revision });
      setMode(kind ? "preview" : "source");
      onSaved();
    } catch (failure) {
      const known = toFailure(failure);
      setError(known.status === 409 ? "This file changed on disk. Reload it before saving." : `Couldn't save. ${known.message}`);
    } finally {
      setSaving(false);
    }
  };
  return (
    <>
      <div className="file-modes segment" role="radiogroup" aria-label="View">
        {kind ? (
          <button type="button" role="radio" aria-checked={mode === "preview"} onClick={() => setMode("preview")}>
            Preview
          </button>
        ) : null}
        <button type="button" role="radio" aria-checked={mode === "source"} onClick={() => setMode("source")}>
          Source
        </button>
        <button type="button" role="radio" aria-checked={mode === "edit"} onClick={() => setMode("edit")}>
          Edit
        </button>
      </div>
      {mode === "preview" && kind ? <FilePreview kind={kind} text={file.content} /> : null}
      {mode === "source" ? <FileComments key={`${threadId}:${cwd}:${path}:${file.revision}`} threadId={threadId} cwd={cwd}
        path={path} revision={file.revision} text={file.content} revealedLine={line} /> : null}
      {mode === "edit" ? (
        <div className="file-editor">
          <textarea aria-label={`Edit ${path}`} value={text} spellCheck={false} onChange={(event) => setText(event.target.value)} />
          {error ? (
            <p className="field-error" role="alert">
              {error}
            </p>
          ) : null}
          <Button small disabled={saving || text === file.content} onClick={() => void save()}>
            Save
          </Button>
        </div>
      ) : null}
    </>
  );
}

/** A file of the thread's checkout, by what it holds. Exported so the bar's own controls can be tested from one file. */
export function Viewer({ threadId, tabId, path, line, onBack }: { threadId: string; tabId: string; path: string; line: number | null; onBack?(): void }) {
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd ?? "");
  const media = mediaKind(path);
  // An archive is read as a folder list rather than as bytes, so it never
  // reaches the hex view even though the service calls the file binary.
  const zip = isZipPath(path);
  // A Word document is read the same way: its own bytes, through the worker.
  const office = officeKind(path);
  const read = useAsync(`file:${JSON.stringify([threadId, cwd, media ? "" : path])}`, async (signal) => {
    const content = media ? null : await readThreadFile(threadId, path, signal);
    if ((useWorkspace.getState().threads[threadId]?.cwd ?? "") !== cwd) throw new DOMException("Checkout changed", "AbortError");
    return { cwd, path, content };
  });
  // Key changes are cleared by an effect; never expose old bytes in that render.
  const file = { ...read, data: read.data?.cwd === cwd && read.data.path === path ? read.data.content : null };
  const pushToast = useUi((state) => state.pushToast);
  const [copying, setCopying] = useState(false);
  // Only a complete, non-binary file has text to copy: a media file is never
  // read here, and a truncated one is not all there is.
  const copyable = file.data && !file.data.binary && !file.data.truncated ? file.data.content : null;
  const copyContents = async () => {
    if (copyable === null) return;
    const clipboard = navigator.clipboard;
    if (!clipboard) {
      pushToast("Couldn't copy. The clipboard is unavailable.");
      return;
    }
    setCopying(true);
    try {
      await clipboard.writeText(copyable);
      pushToast("Contents copied");
    } catch (failure) {
      const reason = failure instanceof Error && failure.message ? failure.message : "The clipboard is unavailable.";
      pushToast(`Couldn't copy. ${reason}`);
    } finally {
      setCopying(false);
    }
  };
  const open = () => openInEditor(threadId, path, line);
  // A plain text file is drawn as the editor (Engineer Designs v106), which has no bar of its own: what it offers is in the header and the tab menu.
  const editing = !media && !zip && !office && !previewKind(path) && !!file.data && !file.data.binary && !file.data.truncated;
  const plain = !media && !zip && !office && !previewKind(path);
  const locked = plain && !!file.data && file.data.binary;
  // Everything but an error, an unfinished read or a file too big to read in full wears the editor's own frame.
  const bare = plain && file.status !== "error" && !file.data?.truncated;
  const setLock = useEditorSessions((state) => state.lock);
  useEffect(() => {
    setLock(tabId, locked);
    return () => setLock(tabId, false);
  }, [tabId, locked, setLock]);
  const openInBrowser = async () => {
    try {
      await openThreadFileInBrowser(threadId, path);
      useUi.getState().showInBrowser("");
    } catch (failure) {
      pushToast(`Couldn't open it. ${toFailure(failure).message}`);
    }
  };
  return (
    <div className="files-viewer" data-thread-id={threadId} data-editor={bare || undefined}>
      {bare ? null : (
      <div className="panel-bar">
        {onBack ? (
          <Button small tone="quiet" onClick={onBack}>
            Files
          </Button>
        ) : null}
        <code title={path}>{path}</code>
        {file.data && (zip || office) ? <span className="zip-size">{formatBytes(file.data.size)}</span> : null}
        <IconButton label="Copy contents" disabled={copyable === null || copying} onClick={() => void copyContents()}>
          <Copy size={14} aria-hidden />
        </IconButton>
        <IconButton
          label="Add file to message"
          disabled={copyable === null}
          onClick={() => {
            useComposerChips.getState().add(threadId, {
              id: chipId(),
              kind: "file",
              path,
              line,
              excerpt: copyable === null ? null : fileExcerpt(copyable, line),
              language: languageForPath(path),
            });
            pushToast("File added to your message");
          }}
        >
          <Paperclip size={14} aria-hidden />
        </IconButton>
        <a className="icon-btn" aria-label="Save file" title="Save file" href={rawUrl(threadId, path, true)} download>
          <Download size={14} aria-hidden />
        </a>
        {/\.x?html?$/i.test(path) ? (
          <IconButton label="Open in browser" onClick={() => void openInBrowser()}>
            <AppWindow size={14} aria-hidden />
          </IconButton>
        ) : null}
        <IconButton label="Open in editor" onClick={() => void open()}>
          <ExternalLink size={14} aria-hidden />
        </IconButton>
      </div>
      )}
      {media ? (
        <FileMedia kind={media} threadId={threadId} path={path} />
      ) : file.status === "loading" ? (
        plain ? <EditorLoading path={path} /> : <Loading label="Loading file" rows={6} />
      ) : file.status === "error" && file.error.status === 404 ? (
        // A moved thread reads its files from the new checkout, which may not have this one.
        <Empty title="This file isn't in this checkout." />
      ) : file.status === "error" ? (
        <ErrorState compact title="Couldn't read this file" failure={file.error} onRetry={file.reload} />
      ) : !file.data ? null : zip ? (
        <ZipViewer threadId={threadId} path={path} size={file.data.size} />
      ) : office ? (
        <DocumentViewer threadId={threadId} path={path} kind={office} size={file.data.size} />
      ) : file.data.binary && plain ? (
        <BinaryFile threadId={threadId} path={path} size={Number(file.data.size)} />
      ) : file.data.binary ? (
        <FileBytes threadId={threadId} path={path} size={file.data.size} as="hex" />
      ) : file.data.truncated ? (
        <FileBytes threadId={threadId} path={path} size={file.data.size} as="text" />
      ) : editing ? (
        <FileEditor threadId={threadId} tabId={tabId} path={path} file={file.data} line={line} onReload={file.reload} />
      ) : (
        <FileBody threadId={threadId} cwd={cwd} path={path} file={file.data} line={line} onSaved={file.reload} />
      )}
    </div>
  );
}
