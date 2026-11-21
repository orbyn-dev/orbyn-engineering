import type { RepoScan, ScannedFolder, TaskFolder, ThreadRepo } from "./contract";

/** What the page keeps of an answer from the service: docs/multi-repo.md returns at most 64 entries, and text is capped again here. */
export const SCAN_ENTRY_CAP = 64;
const TEXT_CAP = 512;

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]+/g;
const text = (value: unknown): string | null => (typeof value === "string" ? value.replace(CONTROL, " ").trim().slice(0, TEXT_CAP) : null);

const SOURCES: ReadonlySet<string> = new Set(["child", "code_workspace"]);
const REFUSALS: ReadonlySet<string> = new Set(["outside", "missing", "not_directory"]);

/** `true` for an object with own keys only: a service answer is data, never read through the prototype. */
const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const own = (record: Record<string, unknown>, key: string): unknown => (Object.hasOwn(record, key) ? record[key] : undefined);

/** One scanned entry, or `null` when it is not one (no name or path as text, an unknown `source`). An unknown `refused` is treated as refused: nothing is added on a reason this page cannot read. */
function cleanFolder(raw: unknown): ScannedFolder | null {
  if (!isRecord(raw)) return null;
  const name = text(own(raw, "name"));
  const path = text(own(raw, "path"));
  const source = own(raw, "source");
  if (!name || !path || typeof source !== "string" || !SOURCES.has(source)) return null;
  const refused = own(raw, "refused");
  return {
    name, path,
    is_git: own(raw, "is_git") === true,
    source: source as ScannedFolder["source"],
    in_project: own(raw, "in_project") === true,
    refused: refused === null || refused === undefined ? null : typeof refused === "string" && REFUSALS.has(refused) ? (refused as ScannedFolder["refused"]) : "outside",
  };
}

/** The service's scan as the page keeps it: shaped, capped at 64 entries, every string single-line and short. `null` when it is not a scan. */
export function cleanScan(raw: unknown): RepoScan | null {
  if (!isRecord(raw)) return null;
  const folders = own(raw, "folders");
  const path = text(own(raw, "path"));
  if (!Array.isArray(folders) || path === null) return null;
  const kept = folders.map(cleanFolder).filter((entry): entry is ScannedFolder => entry !== null);
  return {
    path,
    is_git: own(raw, "is_git") === true,
    code_workspace_file: text(own(raw, "code_workspace_file")),
    folders: kept.slice(0, SCAN_ENTRY_CAP),
    truncated: own(raw, "truncated") === true || kept.length > SCAN_ENTRY_CAP,
  };
}

function cleanThreadRepo(raw: unknown): ThreadRepo | null {
  if (!isRecord(raw)) return null;
  const repo_id = text(own(raw, "repo_id"));
  const name = text(own(raw, "name"));
  const branch = text(own(raw, "branch"));
  if (!repo_id || !name || branch === null) return null;
  const count = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
  return { repo_id, name, branch, added: count(own(raw, "added")), deleted: count(own(raw, "deleted")), trusted: own(raw, "trusted") === true };
}

/** The service's task folder as the page keeps it, or `null` when it is not one. `root` is shown as text only. */
export function cleanTaskFolder(raw: unknown): TaskFolder | null {
  if (!isRecord(raw)) return null;
  const root = text(own(raw, "root"));
  const branch = text(own(raw, "branch"));
  const repos = own(raw, "repos");
  if (!root || !branch || !Array.isArray(repos)) return null;
  const max = own(raw, "max_per_thread");
  return {
    root, branch,
    repos: repos.map(cleanThreadRepo).filter((entry): entry is ThreadRepo => entry !== null).slice(0, 8),
    max_per_thread: typeof max === "number" && Number.isInteger(max) && max > 0 ? max : 8,
  };
}

/** Why a folder cannot be added by a scan, in words. */
export const REFUSED_TEXT: Readonly<Record<NonNullable<ScannedFolder["refused"]>, string>> = {
  outside: "Outside the workspace file's folder. Use Add a folder to pick it yourself.",
  missing: "This folder does not exist.",
  not_directory: "This is a file, not a folder.",
};

/** A folder row can be added when the service did not refuse it and no project has it yet. */
export const canAddScanned = (folder: ScannedFolder): boolean => folder.refused === null && !folder.in_project;

/**
 * A branch name for a new task, by the checks of `git check-ref-format --branch`: no empty parts, no `..`, no control character, space,
 * `~ ^ : ? * [ \`, no `@{`, no leading dash or dot part, no part ending `.lock`, no trailing slash or dot, and not `@`. The service checks it
 * again, and a name already used in any chosen repository is refused there (`409 invalid`).
 */
export function checkBranchName(input: string): { ok: true; name: string } | { ok: false; reason: string } {
  const name = input.trim();
  if (name === "") return { ok: false, reason: "Enter a branch name." };
  if (name.length > 200) return { ok: false, reason: "The branch name is too long." };
  // eslint-disable-next-line no-control-regex
  if (/[\u0000- \u007f~^:?*[\\]/.test(name)) return { ok: false, reason: "A branch name cannot hold spaces or the characters ~ ^ : ? * [ \\." };
  if (name.startsWith("-")) return { ok: false, reason: "A branch name cannot start with a dash." };
  if (name === "@" || name.includes("@{")) return { ok: false, reason: "A branch name cannot be @ or hold @{." };
  if (name.includes("..") || name.includes("//") || name.startsWith("/") || name.endsWith("/") || name.endsWith(".")) return { ok: false, reason: "A branch name cannot hold .. or //, or start or end with a slash, or end with a dot." };
  if (name.split("/").some((part) => part.startsWith(".") || part.endsWith(".lock"))) return { ok: false, reason: "A part of a branch name cannot start with a dot or end with .lock." };
  return { ok: true, name };
}
