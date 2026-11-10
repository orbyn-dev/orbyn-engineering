/** `orbyn://thread/<id>` opens a thread here. */
export const threadLinkId = (href: string): string | null =>
  /^orbyn:\/\/thread\/([\w-]+)$/.exec(href.trim())?.[1] ?? null;

/** A path relative to the checkout or absolute, with an optional line: `src/a.ts:12`. */
export function fileLinkTarget(raw: string): { path: string; line: number | null } | null {
  const href = raw.trim().replace(/^file:\/\//, "");
  if (!href || /^[a-z][a-z0-9+.-]*:\/\//i.test(href) || /^mailto:/i.test(href) || href.startsWith("#")) return null;
  const hash = /#L(\d+)(?:C\d+)?$/.exec(href);
  const colon = /:(\d+)(?::\d+)?$/.exec(href);
  const line = Number(hash?.[1] ?? colon?.[1]) || null;
  const path = decodeURI(href.replace(/#.*$/, "").replace(/:\d+(?::\d+)?$/, ""));
  return /[/.]/.test(path) ? { path, line } : null;
}

export function safeHref(raw: string): string | null {
  const href = raw.trim();
  // Control characters and whitespace can hide a scheme from a naive check.
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\s]/.test(href)) return null;
  try {
    const url = new URL(href);
    return url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:"
      ? url.href
      : null;
  } catch {
    return null;
  }
}
