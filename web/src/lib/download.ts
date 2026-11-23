/** The time the object URL lives after the click: long enough for every browser to start the download, short enough not to linger. */
export const REVOKE_AFTER_MS = 1000;

/** Hands a Blob to the person's browser: an object URL and an anchor, no request, no server. The URL is revoked after. */
export function saveBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  try {
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = name;
    anchor.rel = "noopener";
    anchor.style.display = "none";
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), REVOKE_AFTER_MS);
  }
}

/** Hands a text file to the person's browser, as `saveBlob` does. */
export function saveTextFile(name: string, text: string, type = "text/html;charset=utf-8"): void {
  saveBlob(name, new Blob([text], { type }));
}
