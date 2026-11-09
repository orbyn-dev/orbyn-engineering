import type { ClipboardEvent, MutableRefObject } from "react";
import { request } from "../api";
import {
  CHIP_MIME,
  chipsFromClipboard,
  fileExcerpt,
  languageForPath,
  refillExcerpts,
  type ComposerChip,
} from "../../../product/composer/composerContext";
import { LARGE_PASTE_BYTES } from "./composerInput";

/** Paste in the field: carried chips, files, or a long text kept as an attachment. */
export function handleComposerPaste(
  event: ClipboardEvent<HTMLTextAreaElement>,
  {
    thread,
    shiftPaste,
    addChips,
    upload,
  }: {
    thread: { id: string };
    shiftPaste: MutableRefObject<boolean>;
    addChips(threadId: string, chips: ComposerChip[]): void;
    upload(files: FileList | File[] | null): void;
  },
) {
  const carried = chipsFromClipboard({
    json: event.clipboardData.getData(CHIP_MIME),
    html: event.clipboardData.getData("text/html"),
  });
  if (carried) {
    event.preventDefault();
    void refillExcerpts(carried, async (path) => {
      const found = await request<{ content: string }>(
        "GET",
        `/api/threads/${encodeURIComponent(thread.id)}/files/content?path=${encodeURIComponent(path)}`,
      );
      return { text: fileExcerpt(found.content, null), language: languageForPath(path) };
    }).then((filled) => addChips(thread.id, filled));
    return;
  }
  const files = [...event.clipboardData.files];
  if (files.length > 0) {
    event.preventDefault();
    upload(files);
    return;
  }
  const text = event.clipboardData.getData("text/plain");
  if (text.length >= LARGE_PASTE_BYTES && !shiftPaste.current) {
    event.preventDefault();
    upload([new File([text], "pasted-text.txt", { type: "text/plain" })]);
  }
  shiftPaste.current = false;
}
