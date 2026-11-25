/**
 * A clone source, by the list in docs/multi-repo.md ("Clone and path input"): an `https://` URL, an `ssh://` URL or an scp-style
 * `user@host:path`. Text the person typed or pasted is untrusted, and the service checks it again: a source that starts with `-`
 * (Git reads it as an option), `ext::`, `fd::`, `file://`, a local path (so also `owner/name`), a newline or other control
 * character, or a user name and password in the address is refused here and there.
 */
export function checkCloneSource(text: string): { ok: true; source: string } | { ok: false; reason: string } {
  const source = text.trim();
  if (source === "") return { ok: false, reason: "Enter a Git URL." };
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f\s]/.test(source)) return { ok: false, reason: "The address cannot hold spaces or control characters." };
  if (source.startsWith("-")) return { ok: false, reason: "The address cannot start with a dash." };
  const url = /^(https|ssh):\/\/([^/@]*@)?[^/@\s]+\/?[^\s]*$/.exec(source);
  if (url) {
    const userinfo = url[2]?.slice(0, -1) ?? "";
    return userinfo.includes(":") ? { ok: false, reason: "The address cannot hold a user name and password." } : { ok: true, source };
  }
  if (/^[A-Za-z0-9_][\w.-]*@[A-Za-z0-9][\w.-]*:[^\s:-][^\s]*$/.test(source)) return { ok: true, source };
  return { ok: false, reason: "Use a https or ssh address, or user@host:path." };
}

/** An `owner/name` on GitHub is a clone address too; anything else goes to Git as typed. */
export function cloneAddress(text: string): string {
  const typed = text.trim();
  return /^[\w.-]+\/[\w.-]+$/.test(typed) && !/^\.{1,2}\//.test(typed) ? `https://github.com/${typed}.git` : typed;
}
