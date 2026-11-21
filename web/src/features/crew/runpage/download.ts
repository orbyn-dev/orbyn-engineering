/** The time the object URL lives after the click: long enough for every browser to start the download, short enough not to linger. */
export const REVOKE_AFTER_MS = 1000;

/** Hands a text file to the person's browser: a Blob and an anchor, no request, no server. The object URL is revoked after. */
export function saveTextFile(name: string, text: string, type = "text/html;charset=utf-8"): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
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
