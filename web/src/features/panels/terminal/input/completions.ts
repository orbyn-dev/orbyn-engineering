/**
 * Completions for the terminal input. Everything here is ours: a small table of common commands and flags written for
 * this app, the person's own history, and file names read from the explorer's listing. No external completion data.
 */

export type CompletionKind = "command" | "flag" | "path" | "history";

export interface Completion {
  label: string;
  /** What replaces the word being typed. */
  insert: string;
  detail: string;
  kind: CompletionKind;
}

export interface PathEntry {
  name: string;
  kind: "file" | "directory" | "symlink";
}

export interface CompleteContext {
  /** Earlier commands, oldest first. */
  history: readonly string[];
  /** The entries of a folder, relative to the worktree root and ending in "/" (empty for the root); null when not known. */
  listDir(dir: string): readonly PathEntry[] | null;
  /**
   * The folder the shell is in, relative to the worktree root, "" for the root itself or "a/b/". Relative paths are
   * completed against it. Empty when the shell has not said, or has said a folder outside the worktree.
   */
  cwd?: string;
}

export interface CompletionResult {
  from: number;
  to: number;
  items: Completion[];
}

interface Spec {
  does: string;
  subs?: Record<string, string>;
  flags?: Record<string, string>;
  /** Takes file names. */
  paths?: boolean;
}

export const SPECS: Readonly<Record<string, Spec>> = {
  git: {
    does: "Version control",
    subs: { status: "Show changed files", add: "Stage changes", commit: "Record staged changes", diff: "Show changes", log: "Show history", checkout: "Switch branch or restore files", switch: "Switch branch", branch: "List or create branches", pull: "Fetch and merge", push: "Send commits", stash: "Set changes aside", rebase: "Replay commits", merge: "Join branches", restore: "Restore files", worktree: "Manage worktrees" },
    flags: { "--help": "Show help", "--version": "Show version", "-C": "Run in another folder" },
    paths: true,
  },
  npm: {
    does: "Node package manager",
    subs: { install: "Install packages", run: "Run a package script", test: "Run tests", start: "Start the app", ci: "Clean install from lockfile", ls: "List packages", outdated: "Show old packages" },
    flags: { "--help": "Show help", "--version": "Show version", "--save-dev": "Save as a dev dependency", "--workspace": "Run in a workspace" },
  },
  cargo: {
    does: "Rust build tool",
    subs: { build: "Compile", test: "Run tests", run: "Build and run", check: "Check without building", clippy: "Lint", fmt: "Format code" },
    flags: { "--release": "Optimised build", "--package": "Pick a package", "--workspace": "All packages", "--help": "Show help" },
  },
  ls: { does: "List files", flags: { "-l": "Long format", "-a": "Include hidden files", "-h": "Readable sizes", "-R": "Recurse", "-t": "Newest first" }, paths: true },
  cd: { does: "Change folder", paths: true },
  cat: { does: "Print files", flags: { "-n": "Number lines" }, paths: true },
  grep: { does: "Search text", flags: { "-r": "Recurse", "-n": "Show line numbers", "-i": "Ignore case", "-v": "Invert match", "-l": "List file names only", "-E": "Extended patterns" }, paths: true },
  rm: { does: "Remove files", flags: { "-r": "Recurse", "-f": "Never ask", "-i": "Ask each time" }, paths: true },
  cp: { does: "Copy files", flags: { "-r": "Recurse", "-i": "Ask before overwrite", "-v": "Say what is copied" }, paths: true },
  mv: { does: "Move or rename", flags: { "-i": "Ask before overwrite", "-v": "Say what is moved" }, paths: true },
  mkdir: { does: "Make folders", flags: { "-p": "Make parents too" }, paths: true },
  touch: { does: "Create an empty file", paths: true },
  node: { does: "Run JavaScript", flags: { "--version": "Show version", "--inspect": "Start the inspector", "-e": "Run a string" }, paths: true },
  docker: { does: "Containers", subs: { ps: "List containers", run: "Start a container", build: "Build an image", images: "List images", logs: "Show output", stop: "Stop a container", compose: "Run a compose file" }, flags: { "--help": "Show help" } },
  curl: { does: "Fetch a URL", flags: { "-s": "Quiet", "-L": "Follow redirects", "-o": "Write to a file", "-I": "Headers only", "-X": "Choose the method" } },
  find: { does: "Find files", flags: { "-name": "Match a name", "-type": "Match a kind", "-maxdepth": "Limit depth" }, paths: true },
  echo: { does: "Print text", flags: { "-n": "No trailing newline" } },
  tar: { does: "Archives", flags: { "-x": "Extract", "-c": "Create", "-f": "Archive file", "-z": "gzip", "-v": "Verbose" }, paths: true },
  open: { does: "Open a file or app", paths: true },
  code: { does: "Open in an editor", paths: true },
};

const LIMIT = 8;

interface Word {
  from: number;
  text: string;
  /** The words of this command before the one being typed. */
  before: string[];
}

function wordAt(line: string, cursor: number): Word {
  const head = line.slice(0, cursor);
  const from = Math.max(head.lastIndexOf(" "), head.lastIndexOf("\t"), head.lastIndexOf("\n")) + 1;
  const text = head.slice(from);
  const sinceSeparator = head.slice(0, from);
  const last = [...sinceSeparator.matchAll(/\|\||&&|[|;&(\n]/g)].pop();
  const segment = last ? sinceSeparator.slice((last.index ?? 0) + last[0].length) : sinceSeparator;
  return { from, text, before: segment.split(/\s+/).filter(Boolean) };
}

const startsWith = (value: string, prefix: string) => value.toLowerCase().startsWith(prefix.toLowerCase());

/**
 * Completions for the word before the cursor. With `explicit` (the person pressed Tab) an empty word completes too;
 * otherwise nothing opens until a character is typed.
 */
export function completeAt(line: string, cursor: number, context: CompleteContext, explicit = false): CompletionResult | null {
  const word = wordAt(line, cursor);
  if (word.text === "" && !explicit) return null;
  const [command] = word.before;
  const items: Completion[] = [];
  if (command === undefined) {
    const names = new Set<string>();
    for (const name of Object.keys(SPECS)) if (startsWith(name, word.text)) {
      names.add(name);
      items.push({ label: name, insert: name, detail: SPECS[name]?.does ?? "", kind: "command" });
    }
    for (let at = context.history.length - 1; at >= 0; at -= 1) {
      const name = (context.history[at] ?? "").trim().split(/\s+/)[0] ?? "";
      if (name !== "" && !names.has(name) && startsWith(name, word.text) && !/[=$"'/]/.test(name)) {
        names.add(name);
        items.push({ label: name, insert: name, detail: "From your history", kind: "history" });
      }
    }
  } else {
    const spec = SPECS[command];
    if (word.text.startsWith("-")) {
      for (const [flag, detail] of Object.entries(spec?.flags ?? {})) if (startsWith(flag, word.text)) items.push({ label: flag, insert: flag, detail, kind: "flag" });
    } else {
      if (word.before.length === 1) {
        for (const [sub, detail] of Object.entries(spec?.subs ?? {})) if (startsWith(sub, word.text)) items.push({ label: sub, insert: sub, detail, kind: "command" });
      }
      if (spec?.paths || word.text.includes("/") || word.text.startsWith(".")) items.push(...pathItems(word.text, context));
    }
  }
  return items.length === 0 ? null : { from: word.from, to: cursor, items: items.slice(0, LIMIT) };
}

/**
 * The folder to list for a typed folder `dir` (what comes before the last slash, with its slash), as the root-relative
 * "" or "a/b/", resolved against the shell's folder `cwd`. Null for an absolute or home path, or a path that climbs
 * out of the worktree: the listing is the explorer's, which has no view of anything else.
 */
export function resolveDir(cwd: string, dir: string): string | null {
  if (dir.startsWith("/") || dir.startsWith("~")) return null;
  const names = cwd.split("/").filter(Boolean);
  for (const name of dir.split("/")) {
    if (name === "" || name === ".") continue;
    if (name === "..") {
      if (names.length === 0) return null;
      names.pop();
    } else names.push(name);
  }
  return names.length === 0 ? "" : `${names.join("/")}/`;
}

function pathItems(text: string, context: CompleteContext): Completion[] {
  const slash = text.lastIndexOf("/");
  const dir = resolveDir(context.cwd ?? "", text.slice(0, slash + 1));
  if (dir === null) return [];
  const base = text.slice(slash + 1);
  const entries = context.listDir(dir);
  if (!entries) return [];
  const prefix = text.slice(0, slash + 1);
  return entries
    .filter((entry) => startsWith(entry.name, base) && (base.startsWith(".") || !entry.name.startsWith(".")))
    .map((entry) => {
      const folder = entry.kind === "directory";
      const insert = `${prefix}${entry.name}${folder ? "/" : ""}`;
      return { label: `${entry.name}${folder ? "/" : ""}`, insert, detail: folder ? "Folder" : entry.kind === "symlink" ? "Link" : "File", kind: "path" as const };
    });
}

/** The line with a completion put in place of the word it completes, and where the cursor goes. */
export function applyCompletion(line: string, result: CompletionResult, item: Completion): { line: string; cursor: number } {
  const tail = line.slice(result.to);
  // A folder stays open for the next name; anything else gets a space unless one follows.
  const space = item.insert.endsWith("/") || tail.startsWith(" ") ? "" : " ";
  const head = `${line.slice(0, result.from)}${item.insert}${space}`;
  return { line: head + tail, cursor: head.length };
}
