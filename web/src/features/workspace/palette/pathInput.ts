/** What the add-project path field shows and means, on either platform's paths. */

const SEP = /[\\/]/;
const DRIVE_ROOT = /^[A-Za-z]:[\\/]$/;

const lastSep = (value: string) => Math.max(value.lastIndexOf("/"), value.lastIndexOf("\\"));

/** A path that means the same thing wherever it starts: POSIX, drive, UNC or home. */
const startsFresh = (path: string) =>
  path === "~" || /^[\\/]/.test(path) || /^[A-Za-z]:[\\/]/.test(path) || /^~[\\/]/.test(path);

/**
 * The field starts on `~/`, so typing `/` or pasting a whole path lands after it.
 * Such a path replaces that prefix instead of nesting under home.
 */
export function replaceHomePrefix(value: string): string {
  const rest = value.replace(/^\s+/, "");
  if (!/^~[\\/]/.test(rest)) return rest;
  const after = rest.slice(2);
  return startsFresh(after) ? after : rest;
}

export function splitPath(value: string): { dir: string; leaf: string } {
  if (value === "~") return { dir: "~/", leaf: "" };
  const at = lastSep(value);
  return { dir: value.slice(0, at + 1), leaf: value.slice(at + 1) };
}

const separatorOf = (path: string) => (path.includes("\\") && !path.includes("/") ? "\\" : "/");

export const withSlash = (path: string) => (SEP.test(path.slice(-1)) ? path : `${path}${separatorOf(path)}`);

/** The `~` root in the home folder's own style: `~\Documents` on Windows, `~/Documents` elsewhere. */
export const homePrefix = (home: string) => `~${separatorOf(home)}`;

export const shortenHome = (path: string, home: string) => {
  const prefix = withSlash(home);
  if (path === home || path === prefix) return homePrefix(home);
  return path.startsWith(prefix) ? `${homePrefix(home)}${path.slice(prefix.length)}` : path;
};

/** The name a path ends in, for the row that goes to the folder above: a root keeps its own text. */
export function pathName(path: string): string {
  const trimmed = dropTrailingSlash(path);
  const at = lastSep(trimmed);
  const name = at < 0 ? trimmed : trimmed.slice(at + 1);
  return name === "" ? trimmed : name;
}

export const expandHome = (path: string, home: string) =>
  path === "~" || /^~[\\/]/.test(path) ? `${withSlash(home)}${path.slice(2)}` : path;

/** The folder a typed path names. A root keeps its slash: `C:` alone is not `C:\`. */
export function dropTrailingSlash(path: string): string {
  return path.length > 1 && !DRIVE_ROOT.test(path) ? path.replace(/[\\/]+$/, "") : path;
}
