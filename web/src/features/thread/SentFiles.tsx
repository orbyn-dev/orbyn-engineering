import { FileCode, FileText } from "lucide-react";
import { use, useEffect, useState } from "react";
import { attachmentBlob } from "./api";
import { toFailure } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Dialog } from "../../ui/Dialog";
import { TimelineThread } from "./timelineThread";
import { MediaDialog, type MediaItem } from "../content";
import { fileLinkTarget } from "../../lib/links";
import type { SentAttachment } from "../../product/timeline/types";

/** A typed `@path` the message referred to. */
const PATH_TOKEN = /(^|\s)@([^\s@]+)/g;
const TRAILING = /[.,;:!?)\]]+$/;
/** Text longer than this is downloaded instead of shown. */
const PREVIEW_BYTES = 256 * 1024;

/** Every distinct path a user typed as an `@` mention. */
export function sentPaths(text: string): string[] {
  const found: string[] = [];
  for (const match of text.matchAll(PATH_TOKEN)) {
    const path = match[2]?.replace(TRAILING, "") ?? "";
    if (!path || !/[/.]/.test(path) || found.includes(path)) continue;
    found.push(path);
  }
  return found;
}

const basename = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

const sizeLabel = (bytes: number) =>
  bytes < 1024
    ? `${bytes} B`
    : bytes < 1_048_576
      ? `${Math.round(bytes / 1024)} KiB`
      : `${(bytes / 1_048_576).toFixed(1)} MiB`;

const textual = (mime: string) =>
  /^text\//.test(mime) ||
  /^application\/(json|xml|yaml|x-yaml|toml|sql|javascript|typescript|x-sh)$/.test(mime);

interface Shown {
  name: string;
  body: string;
}

function download(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

const isImage = (file: SentAttachment) => file.mime.startsWith("image/");

/** The bytes of each sent image as a blob URL, loaded once, until the message goes. */
function useImageUrls(images: readonly SentAttachment[]): Record<string, string | null> {
  const [urls, setUrls] = useState<Record<string, string | null>>({});
  const key = images.map((file) => file.id).join(",");
  useEffect(() => {
    let live = true;
    const made: string[] = [];
    setUrls({});
    for (const file of images) {
      attachmentBlob(file.id).then(
        (blob) => {
          if (!live) return;
          const url = URL.createObjectURL(blob);
          made.push(url);
          setUrls((current) => ({ ...current, [file.id]: url }));
        },
        () => live && setUrls((current) => ({ ...current, [file.id]: null })),
      );
    }
    return () => {
      live = false;
      for (const url of made) URL.revokeObjectURL(url);
    };
    // The ids are the dependency: a new array of the same files must not reload them.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return urls;
}

/** What a sent user message carried: files by name and paths by mention. */
export function SentFiles({
  attachments,
  text,
}: {
  attachments?: readonly SentAttachment[];
  text: string;
}) {
  const threadId = use(TimelineThread);
  const [shown, setShown] = useState<Shown | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<{ index: number; opener: HTMLElement } | null>(null);
  const files = attachments ?? [];
  const images = files.filter(isImage);
  const chips = files.filter((file) => !isImage(file));
  const urls = useImageUrls(images);
  const loaded: MediaItem[] = images.flatMap((file) => {
    const src = urls[file.id];
    return src ? [{ kind: "image" as const, src, alt: file.name }] : [];
  });

  const open = async (file: SentAttachment) => {
    setError(null);
    setBusy(file.id);
    try {
      const blob = await attachmentBlob(file.id);
      if (textual(file.mime) && blob.size <= PREVIEW_BYTES) {
        setShown({ name: file.name, body: await blob.text() });
      } else {
        download(blob, file.name);
      }
    } catch (failure) {
      setError(toFailure(failure).message);
    } finally {
      setBusy(null);
    }
  };

  const reveal = (path: string) => {
    const target = fileLinkTarget(path);
    if (!target) return;
    const thread = threadId ? useWorkspace.getState().threads[threadId] : null;
    if (thread) useUi.getState().revealFile(target.path, target.line, { threadId: thread.id, cwd: thread.cwd });
  };

  const paths = sentPaths(text);
  if (files.length === 0 && paths.length === 0) return null;

  return (
    <>
      {images.length > 0 ? (
        <ul className="sent-gallery" aria-label="Sent with this message" data-many={images.length > 6 || undefined}>
          {images.map((file) => {
            const src = urls[file.id];
            const at = loaded.findIndex((item) => item.src === src);
            return (
              <li key={`image:${file.id}`}>
                {src ? (
                  <button
                    type="button"
                    aria-label={`Expand ${file.name}`}
                    title={`${file.name} · ${sizeLabel(file.size)}`}
                    onClick={(event) => setExpanded({ index: at, opener: event.currentTarget })}
                  >
                    <img src={src} alt={file.name} />
                  </button>
                ) : (
                  <span className="sent-gallery-wait" title={file.name}>
                    {src === null ? `Image unavailable · ${file.name}` : ""}
                  </span>
                )}
                {file.source ? (
                  <small className="sent-snapshot-source">Snap Shot · {file.source.app_name} · {file.source.window_title}</small>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
      {chips.length > 0 || paths.length > 0 ? (
      <ul className="sent-files" aria-label={images.length > 0 ? "Files sent with this message" : "Sent with this message"}>
        {chips.map((file) => (
          <li key={`file:${file.id}`}>
            <button
              type="button"
              disabled={busy === file.id}
              aria-label={`Open ${file.name}`}
              title={`${file.name} · ${sizeLabel(file.size)}`}
              onClick={() => void open(file)}
            >
              <FileText size={12} aria-hidden />
              <span>{file.name}</span>
            </button>
          </li>
        ))}
        {paths.map((path) => (
          <li key={`path:${path}`}>
            <button
              type="button"
              className="sent-path"
              aria-label={`Show ${path}`}
              title={path}
              onClick={() => reveal(path)}
            >
              <FileCode size={12} aria-hidden />
              <span>{basename(path)}</span>
            </button>
          </li>
        ))}
      </ul>
      ) : null}
      {error ? (
        <p className="sent-files-error" role="alert">
          {error}
        </p>
      ) : null}
      {expanded ? (
        <MediaDialog items={loaded} index={expanded.index} opener={expanded.opener} onClose={() => setExpanded(null)} />
      ) : null}
      {shown ? (
        <Dialog title={shown.name} onClose={() => setShown(null)}>
          <pre className="sent-preview-text">{shown.body}</pre>
        </Dialog>
      ) : null}
    </>
  );
}
