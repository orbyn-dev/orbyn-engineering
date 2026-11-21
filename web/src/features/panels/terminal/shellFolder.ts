/**
 * The shell's current folder, as the shell reports it with OSC 7 (`ESC ] 7 ; file://host/path BEL`). The text comes from
 * the shell or a program in it, so it is untrusted: it is parsed strictly, kept only as data, never shown and never logged.
 * Nothing here lists a folder; the editor lists a reported folder only when it lies inside the thread's own root.
 */
import { useSyncExternalStore } from "react";

const MAX_PATH = 4096;
// A raw report may be longer than its path (the host, percent escapes); past this it is not read at all.
const MAX_REPORT = MAX_PATH * 3 + 512;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

/**
 * The absolute path in an OSC 7 report, or null. Only `file://` is read; the host is ignored (a shell may name this
 * machine in any way). A path with a control character, a bad percent escape or more than 4096 characters is refused.
 */
export function parseOsc7(data: string): string | null {
  if (data.length > MAX_REPORT || CONTROL.test(data)) return null;
  const match = /^file:\/\/([^/]*)(\/.*)$/i.exec(data);
  if (!match) return null;
  let path: string;
  try {
    path = decodeURIComponent(match[2] ?? "");
  } catch {
    return null;
  }
  return path.length === 0 || path.length > MAX_PATH || CONTROL.test(path) ? null : path;
}

/** Brings a root or a reported path to one shape: forward slashes, no leading slash before a drive letter. */
const unified = (path: string): string => path.replace(/\\/g, "/").replace(/^\/([A-Za-z]:(\/|$))/, "$1");

/**
 * Where `reported` is relative to the root `root`: "" for the root itself, otherwise "a/b/" (a trailing slash). Null when
 * the folder is not inside the root, or when either is not a plain path. `..` in the report is resolved, and a report
 * that climbs out of the root is refused.
 */
export function folderInRoot(root: string, reported: string): string | null {
  const base = unified(root);
  if (base === "") return null;
  const rootNames = base.split("/").filter((name) => name !== "" && name !== ".");
  if (rootNames.includes("..")) return null;
  const names: string[] = [];
  for (const name of unified(reported).split("/")) {
    if (name === "" || name === ".") continue;
    if (name === "..") {
      if (names.length === 0) return null;
      names.pop();
    } else names.push(name);
  }
  const same = (a: string | undefined, b: string | undefined, at: number) => (at === 0 && /^[A-Za-z]:$/.test(a ?? "") ? a?.toLowerCase() === b?.toLowerCase() : a === b);
  if (names.length < rootNames.length || !rootNames.every((name, at) => same(name, names[at], at))) return null;
  const rest = names.slice(rootNames.length);
  return rest.length === 0 ? "" : `${rest.join("/")}/`;
}

const folders = new Map<string, string>();
const listeners = new Map<string, Set<() => void>>();

const tell = (key: string) => {
  for (const listener of listeners.get(key) ?? []) listener();
};

/** The folder the shell of one terminal last reported, null before it has. */
export const shellFolderOf = (key: string): string | null => folders.get(key) ?? null;

export function setShellFolder(key: string, path: string): void {
  if (folders.get(key) === path) return;
  folders.set(key, path);
  tell(key);
}

export function clearShellFolder(key: string): void {
  if (folders.delete(key)) tell(key);
}

export function subscribeShellFolder(key: string, listener: () => void): () => void {
  const set = listeners.get(key) ?? new Set();
  listeners.set(key, set);
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0 && listeners.get(key) === set) listeners.delete(key);
  };
}

/** The reported folder of a terminal, for rendering. */
export function useShellFolder(key: string): string | null {
  return useSyncExternalStore(
    (listener) => subscribeShellFolder(key, listener),
    () => shellFolderOf(key),
    () => null,
  );
}

/** The part of xterm's parser this needs. */
export interface OscParser {
  registerOscHandler(ident: number, handler: (data: string) => boolean): { dispose(): void };
}

/**
 * Records this terminal's reports. A report that does not parse is consumed and changes nothing. Disposing stops listening
 * and forgets the folder, so a closed terminal leaves nothing behind.
 */
export function hearShellFolder(parser: OscParser, key: string): { dispose(): void } {
  const handler = parser.registerOscHandler(7, (data) => {
    const path = parseOsc7(data);
    if (path !== null) setShellFolder(key, path);
    return true;
  });
  return {
    dispose() {
      handler.dispose();
      clearShellFolder(key);
    },
  };
}
