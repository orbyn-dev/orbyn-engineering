import { prettyJson } from "../../lib/format";
import { outputSpans } from "./ansi";
import type { ToolItem } from "./projection";
import { baseName, parsed, paths, toolFailed, traceTool } from "./toolTrace";

/** The kind of row a tool call is drawn as; each has its own icon, words and block. */
export type CallKind =
  | "command"
  | "read"
  | "edit"
  | "create"
  | "search"
  | "list"
  | "fetch"
  | "mcp"
  | "subagent"
  | "image"
  | "web"
  | "browser"
  | "other";

const IMAGE = /^image_view/i;
const MCP = /^mcp[_.:]/i;
const SUBAGENT = /^(task|agent|subagent)$/i;
const LIST = /^(ls|list_?dir|list_?files?)$/i;
const SEARCH = /^(grep|glob|find|search_files)/i;
const FETCH = /^web_?fetch/i;
const WRITE = /^write$/i;

export function callKind(item: Pick<ToolItem, "name" | "args">): CallKind {
  const name = item.name;
  if (MCP.test(name)) return "mcp";
  if (SUBAGENT.test(name)) return "subagent";
  if (IMAGE.test(name)) return "image";
  if (FETCH.test(name)) return "fetch";
  if (LIST.test(name)) return "list";
  if (SEARCH.test(name)) return "search";
  if (WRITE.test(name)) return "create";
  const { action } = traceTool(item);
  switch (action) {
    case "command":
      return "command";
    case "edit":
      return "edit";
    case "read":
      return "read";
    case "web":
      return "web";
    case "browser":
      return "browser";
    default:
      return "other";
  }
}

/** A short run of the same kind reads as one group; the rest are rows of their own. */
const GROUPED: readonly CallKind[] = ["command", "read", "edit"];
export const groupsKind = (kind: CallKind) => GROUPED.includes(kind);

/** Seconds as the rows show them: 0.1s, 4.2s, 41s, 1m 02s. */
export function callDuration(ms: number): string {
  const tenths = Math.max(0, Math.round(ms / 100));
  if (tenths < 100) return `${Math.floor(tenths / 10)}.${tenths % 10}s`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
}

const stamp = (iso: string) => new Date(iso).getTime();

/** How long a call took, or how long it has run so far; null when no time is known. */
export function callMs(item: ToolItem, now: number | null): number | null {
  const start = stamp(item.ts);
  if (Number.isNaN(start)) return null;
  if (item.endedAt) {
    const end = stamp(item.endedAt);
    return Number.isNaN(end) ? null : Math.max(end - start, 0);
  }
  return now === null ? null : Math.max(now - start, 0);
}

export type CallState = "running" | "done" | "failed" | "stopped";

export function callState(item: ToolItem, live: boolean, cancelled: boolean): CallState {
  const finished = item.result !== null || item.done;
  if (toolFailed(item)) return "failed";
  if (!finished) return live ? "running" : cancelled ? "stopped" : "done";
  return "done";
}

const text = (body: Record<string, unknown>, ...keys: string[]): string | null => {
  for (const key of keys) {
    const value = body[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return null;
};

const count = (body: Record<string, unknown>, key: string): number | null => {
  const value = body[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
};

/** The folder a path is shown relative to, when it sits inside it. */
export function shownPath(path: string, cwd: string | null): string {
  if (cwd && path.startsWith(`${cwd.replace(/\/$/, "")}/`)) return path.slice(cwd.replace(/\/$/, "").length + 1);
  return path;
}

const linesOf = (value: string): string[] => (value === "" ? [] : value.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n"));

export interface DiffStat {
  added: number;
  removed: number;
}

interface Replacement {
  old: string;
  next: string;
}

function replacements(body: Record<string, unknown>): Replacement[] {
  const edits = body.edits;
  if (Array.isArray(edits)) {
    return edits.flatMap((entry): Replacement[] => {
      const one = entry && typeof entry === "object" ? (entry as Record<string, unknown>) : {};
      const old = text(one, "old_string");
      const next = typeof one.new_string === "string" ? one.new_string : null;
      return old !== null && next !== null ? [{ old, next }] : [];
    });
  }
  const old = typeof body.old_string === "string" ? body.old_string : null;
  const next = typeof body.new_string === "string" ? body.new_string : null;
  return old !== null && next !== null ? [{ old, next }] : [];
}

/** The lines an edit removed and added, read from its arguments. */
export function editDiff(item: Pick<ToolItem, "args">): { removed: string[]; added: string[] } | null {
  const list = replacements(parsed(item.args));
  if (list.length === 0) return null;
  return { removed: list.flatMap((entry) => linesOf(entry.old)), added: list.flatMap((entry) => linesOf(entry.next)) };
}

export function editStat(item: Pick<ToolItem, "args" | "name">): DiffStat | null {
  const body = parsed(item.args);
  if (callKind({ name: item.name, args: item.args }) === "create") {
    const content = typeof body.content === "string" ? body.content : null;
    return content === null ? null : { added: linesOf(content).length, removed: 0 };
  }
  const diff = editDiff(item);
  return diff ? { added: diff.added.length, removed: diff.removed.length } : null;
}

/** The first path a call touched. */
export const callPath = (item: Pick<ToolItem, "args">): string | null => paths(parsed(item.args))[0] ?? null;

export function readRange(item: Pick<ToolItem, "args">): { from: number; to: number | null } | null {
  const body = parsed(item.args);
  const offset = count(body, "offset") ?? count(body, "start_line");
  const limit = count(body, "limit");
  const end = count(body, "end_line");
  if (offset === null && limit === null && end === null) return null;
  const from = Math.max(offset ?? 1, 1);
  return { from, to: end ?? (limit !== null ? from + limit - 1 : null) };
}

const NUMBERED = /^\s*(\d+)(?:→|\t)(.*)$/;

export interface SourceLine {
  n: number | null;
  text: string;
}

/** A read result as numbered lines; `cat -n` style output carries its own numbers. */
export function sourceLines(result: string | null, from: number): SourceLine[] {
  const lines = linesOf(result ?? "");
  const numbered = lines.filter((line) => NUMBERED.test(line)).length;
  if (lines.length > 0 && numbered * 2 >= lines.length) {
    return lines.map((line) => {
      const match = NUMBERED.exec(line);
      return match ? { n: Number(match[1]), text: match[2] ?? "" } : { n: null, text: line };
    });
  }
  return lines.map((line, at) => ({ n: from + at, text: line }));
}

export interface ResultRow {
  where: string;
  line: number | null;
  text: string;
}

const LOCATED = /^(.+?):(\d+):(.*)$/;
const NO_RESULTS = /^(no (files|matches|results) found|0 (files|matches))/i;

/** Search output as path:line rows; a plain list of paths becomes rows with no text. */
export function resultRows(result: string | null): ResultRow[] {
  const rows: ResultRow[] = [];
  for (const line of linesOf(result ?? "")) {
    const trimmed = line.trim();
    if (!trimmed || NO_RESULTS.test(trimmed) || /^found \d+ (files?|matches|results?)/i.test(trimmed)) continue;
    const located = LOCATED.exec(trimmed);
    rows.push(located ? { where: `${located[1]}:${located[2]}`, line: Number(located[2]), text: (located[3] ?? "").trim() } : { where: trimmed, line: null, text: "" });
  }
  return rows;
}

/** A listing as names, with the tree markers some engines add taken off. */
export function listEntries(result: string | null): string[] {
  return linesOf(result ?? "")
    .map((line) => line.replace(/^[\s\-*•]+/, "").trimEnd())
    .filter((line) => line !== "" && !NO_RESULTS.test(line) && !/^NOTE:/i.test(line));
}

export const pluralize = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export interface CallLabel {
  /** Plain words, drawn in the quiet colour. */
  verb: string | null;
  /** A command, path or address, drawn in mono. */
  object: string | null;
  /** The mono tail after a dot. */
  note: string | null;
  /** The object takes the rest of the row (a command). */
  fill: boolean;
}

export const hostOf = (url: string): string => {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
};

/** Server and tool of an MCP call named `mcp__server__tool` or `mcp.server.tool`. */
export function mcpParts(name: string): { server: string; tool: string } {
  const parts = name.split(/__|[.:]/).filter(Boolean);
  if (parts.length >= 3) return { server: parts[1] ?? name, tool: parts.slice(2).join("_") };
  if (parts.length === 2) return { server: parts[0] ?? name, tool: parts[1] ?? name };
  return { server: name, tool: "" };
}

const firstScalar = (body: Record<string, unknown>): string | null => {
  for (const value of Object.values(body)) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
};

const sign = (stat: DiffStat | null) => (stat ? `+${stat.added} −${stat.removed}` : null);

/** What the row says, split into the parts the design colours differently. */
export function callLabel(item: ToolItem, cwd: string | null): CallLabel {
  const body = parsed(item.args);
  const kind = callKind(item);
  const one = callPath(item);
  const shown = one ? shownPath(one, cwd) : null;
  const trace = traceTool(item);
  switch (kind) {
    case "command":
      return { verb: null, object: trace.label, note: null, fill: true };
    case "read": {
      const range = readRange(item);
      const files = paths(body);
      if (files.length > 1) return { verb: `Read ${files.length} files`, object: null, note: null, fill: false };
      return {
        verb: "Read",
        object: shown ?? "a file",
        note: range ? `· lines ${range.from}${range.to !== null ? `–${range.to}` : ""}` : null,
        fill: false,
      };
    }
    case "edit": {
      const files = paths(body);
      const stat = sign(editStat(item));
      if (files.length > 1) return { verb: `Edited ${files.length} files`, object: null, note: stat, fill: false };
      return { verb: "Edited", object: shown ?? "files", note: stat, fill: false };
    }
    case "create": {
      const created = item.result === null || /created successfully/i.test(item.result);
      const stat = editStat(item);
      return { verb: created ? "Created" : "Edited", object: shown ?? "a file", note: stat ? `+${stat.added}` : null, fill: false };
    }
    case "search": {
      const query = text(body, "pattern", "query") ?? text(body, "path");
      const rows = item.result !== null ? resultRows(item.result).length : null;
      return {
        verb: query ? `Searched for “${query}”` : "Searched code",
        object: null,
        note: rows === null ? null : `· ${pluralize(rows, "result", "results")}`,
        fill: false,
      };
    }
    case "list": {
      const entries = item.result !== null ? listEntries(item.result).length : null;
      return {
        verb: "Listed",
        object: shown ?? "a folder",
        note: entries === null ? null : `· ${pluralize(entries, "entry", "entries")}`,
        fill: false,
      };
    }
    case "fetch": {
      const url = text(body, "url");
      return { verb: "Fetched", object: url ? hostOf(url) : "a page", note: null, fill: false };
    }
    case "mcp": {
      const { server, tool } = mcpParts(item.name);
      const first = firstScalar(body);
      return { verb: server, object: tool || null, note: first ? `· “${first}”` : null, fill: false };
    }
    case "subagent": {
      const who = text(body, "subagent_type", "description", "name");
      return { verb: "Subagent", object: who, note: null, fill: false };
    }
    case "image": {
      return { verb: "Viewed", object: one ? baseName(one) : "an image", note: null, fill: false };
    }
    default:
      return { verb: trace.label, object: null, note: null, fill: false };
  }
}

/** The parts of a call's block that find counts: exactly the strings the block renders as text. */
export function callFindText(item: ToolItem): string[] {
  const kind = callKind(item);
  const body = parsed(item.args);
  const output = outputSpans(item.result ?? item.output).map((span) => span.text).join("");
  switch (kind) {
    case "command":
      return [traceTool(item).command || prettyJson(item.args), output].filter(Boolean);
    case "read": {
      const range = readRange(item);
      return [...sourceLines(item.result, range?.from ?? 1).map((line) => line.text)].filter(Boolean);
    }
    case "edit": {
      const diff = editDiff(item);
      return diff ? [...diff.removed, ...diff.added] : [prettyJson(item.args), output].filter(Boolean);
    }
    case "create": {
      const content = typeof body.content === "string" ? body.content : null;
      return content === null ? [prettyJson(item.args), output].filter(Boolean) : linesOf(content);
    }
    case "search":
      return resultRows(item.result ?? item.output).flatMap((row) => [row.where, row.text].filter(Boolean));
    case "list":
      return listEntries(item.result ?? item.output);
    case "fetch":
      return [text(body, "url"), output].filter((part): part is string => Boolean(part));
    case "mcp":
      return [...Object.values(body).map(scalar), output].filter(Boolean);
    case "subagent":
      return [text(body, "description", "prompt"), output].filter((part): part is string => Boolean(part));
    default:
      return [prettyJson(item.args), output].filter(Boolean);
  }
}

/** An argument value as a key–value row shows it. */
export function scalar(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "";
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

export const argEntries = (item: Pick<ToolItem, "args">): [string, string][] =>
  Object.entries(parsed(item.args)).map(([key, value]) => [key, scalar(value)]);

/** Lines of output, counted the way the block's "Show all N lines" says. */
export const lineCount = (value: string): number => linesOf(value).length;

export const LINES_SHOWN = 12;
export const ROWS_SHOWN = 7;
export const ENTRIES_SHOWN = 6;
export const WIDE_LINE = 100;

export const hasWideLine = (value: string): boolean => linesOf(value).some((line) => line.length > WIDE_LINE);

/** Consecutive same-kind calls; reasoning and rows that never group stand alone. */
export interface CallRun {
  key: string;
  kind: CallKind;
  items: ToolItem[];
}

export function callRuns(items: readonly ToolItem[]): CallRun[] {
  const runs: CallRun[] = [];
  for (const item of items) {
    const kind = callKind(item);
    const last = runs.at(-1);
    if (last && groupsKind(kind) && last.kind === kind) last.items.push(item);
    else runs.push({ key: item.key, kind, items: [item] });
  }
  return runs;
}

export interface RunLabel {
  verb: string;
  note: string | null;
}

export function runLabel(run: CallRun, running: boolean): RunLabel {
  const n = run.items.length;
  switch (run.kind) {
    case "command":
      return { verb: running ? (n === 1 ? "Running command" : `Running ${n} commands`) : `Ran ${pluralize(n, "command", "commands")}`, note: null };
    case "read":
      return { verb: `Read ${pluralize(n, "file", "files")}`, note: null };
    case "edit": {
      const files = new Set(run.items.flatMap((item) => paths(parsed(item.args))));
      const stats = run.items.map(editStat);
      const total = stats.reduce<DiffStat | null>(
        (sum, stat) => (stat ? { added: (sum?.added ?? 0) + stat.added, removed: (sum?.removed ?? 0) + stat.removed } : sum),
        null,
      );
      return { verb: `Edited ${pluralize(Math.max(files.size, n), "file", "files")}`, note: sign(total) };
    }
    default:
      return { verb: String(n), note: null };
  }
}
