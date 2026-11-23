/** Folder paths as text, for a surface that has no listing yet: both platforms' separators, drive, UNC and POSIX roots. */

const DRIVE_ROOT = /^[A-Za-z]:[\\/]$/;

const lastSep = (value: string) => Math.max(value.lastIndexOf("/"), value.lastIndexOf("\\"));

/** Without its trailing separators, except a root: `C:\` and `/` name folders, `C:` alone does not. */
const withoutTrailingSep = (path: string) => (path === "/" || DRIVE_ROOT.test(path) ? path : path.replace(/[\\/]+$/, ""));

/**
 * The folder that holds this one, read from the path's own text, so Up still works
 * while the listing it belongs to loads. A root holds itself: null.
 */
export function parentPath(path: string): string | null {
  const current = withoutTrailingSep(path);
  if (current === "/" || DRIVE_ROOT.test(current)) return null;
  const at = lastSep(current);
  if (at < 0) return null;
  const above = current.slice(0, at);
  // "/Users" -> "/": the root keeps its own separator instead of becoming empty.
  if (above === "") return current.slice(0, 1);
  // "C:\Users" -> "C:\": a drive name alone is not a folder.
  if (/^[A-Za-z]:$/.test(above)) return `${above}${current[at]}`;
  return above;
}
