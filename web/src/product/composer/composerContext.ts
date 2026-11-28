import { pullToken, threadToken } from "./skillMention";

/**
 * Context a message carries as a chip instead of typed text: a file, a terminal
 * selection, a review comment, a picked page element, a cited reply, a thread
 * picked from the @ menu, or a pull request picked from the # menu.
 *
 * A chip is kept in the composer's own state. When the message is sent, each
 * chip becomes a readable block at the top of the prompt, so the engine sees
 * exactly what the person attached and the transcript shows it.
 */

export type ChipKind = "file" | "terminal" | "review" | "annotation" | "citation" | "thread" | "pull";

interface ChipBase {
  id: string;
  kind: ChipKind;
}

export interface FileChip extends ChipBase {
  kind: "file";
  path: string;
  line: number | null;
  /** The last line of a selected range, when the chip names more than one line. */
  endLine?: number;
  /** Another repository of a multi-repository thread, whose worktree holds `path`; absent for the thread's own checkout and for its primary repository. */
  repoId?: string;
  /** That repository's name, so the file says where it is; set for a repository of a multi-repository thread, the primary's too. */
  repoName?: string;
  /** Selected in the Changes diff, so the chip can take the person back to it there. */
  fromDiff?: boolean;
  /** The file's text at the time it was attached; null until read. */
  excerpt: string | null;
  language: string | null;
}

export interface TerminalChip extends ChipBase {
  kind: "terminal";
  title: string;
  output: string;
  /** One-based buffer rows the selection covered, when the terminal reported them. */
  lineStart: number | null;
  lineEnd: number | null;
  /** The command whose output was selected, when the shell marked it (OSC 133/633). */
  command?: string;
  /** Its exit code: null while it still ran; absent when the shell marked none. */
  exitCode?: number | null;
}

export interface ReviewChip extends ChipBase {
  kind: "review";
  path: string;
  line: number | null;
  /** Optional for older one-line diff comments. */
  endLine?: number;
  /** Another repository of a multi-repository thread, whose worktree holds `path`; absent for the thread's own checkout and for its primary repository. */
  repoId?: string;
  /** The name of that repository, or of the thread's primary repository, so the comment says where the file is; set for a repository of a multi-repository thread, the primary's too. */
  repoName?: string;
  /** File annotations belong to the displayed source snapshot, not a later read. */
  sourceRevision?: string;
  sourceCwd?: string;
  /** The file was edited and the text this comment pointed at is gone or changed: line and excerpt are as first captured. */
  stale?: boolean;
  comment: string;
  excerpt: string | null;
}

export interface AnnotationChip extends ChipBase {
  kind: "annotation";
  url: string;
  selector: string;
  text: string;
  html: string;
}

export interface CitationChip extends ChipBase {
  kind: "citation";
  threadId: string;
  text: string;
}

/** A thread picked from the @ menu, read by the agent on demand. */
export interface ThreadChip extends ChipBase {
  kind: "thread";
  threadId: string;
  title: string;
}

/** A pull request picked from the # menu, carried as its own link. */
export interface PullChip extends ChipBase {
  kind: "pull";
  number: number;
  title: string;
  url: string;
}

export type ComposerChip = FileChip | TerminalChip | ReviewChip | AnnotationChip | CitationChip | ThreadChip | PullChip;

export const CHIP_MIME = "application/x-orbyn-composer+json";
export const CHIP_ATTRIBUTE = "data-orbyn-composer";
/** The most a single chip adds to the prompt or the clipboard. */
export const CHIP_EXCERPT_LIMIT = 4000;
/** The service refuses longer messages; the composer says so before sending. */
export const MAX_PROMPT_CHARS = 120_000;
export const charactersOver = (text: string): number =>
  text.length <= MAX_PROMPT_CHARS ? 0 : Math.max([...text].length - MAX_PROMPT_CHARS, 0);

export const chipId = (): string =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `chip-${Math.random().toString(36).slice(2)}`;

const oneLine = (text: string) => text.replace(/\s+/g, " ").trim();

const excerptOf = (text: string) =>
  text.length <= CHIP_EXCERPT_LIMIT ? text : `${text.slice(0, CHIP_EXCERPT_LIMIT)}\n…`;

/** A fence inside the excerpt must not end the block that carries it. */
const fenceSafe = (text: string) => text.replaceAll("```", "` ` `");

const quote = (text: string) =>
  text
    .split("\n")
    .map((line) => (line ? `> ${line}` : ">"))
    .join("\n");

const terminalLabel = (chip: TerminalChip): string => {
  const title = chip.title || "Terminal selection";
  if (chip.lineStart === null || chip.lineEnd === null) return title;
  return chip.lineStart === chip.lineEnd
    ? `${title} line ${chip.lineStart}`
    : `${title} lines ${chip.lineStart}–${chip.lineEnd}`;
};

/** Inline code that survives backticks in the text it holds. */
const inlineCode = (text: string): string => {
  const longest = Math.max(0, ...(text.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(longest + 1);
  const pad = text.startsWith("`") || text.endsWith("`") ? " " : "";
  return `${fence}${pad}${text}${pad}${fence}`;
};

/** The command line and exit a shell marked for a terminal selection, if any. */
const terminalCommand = (chip: TerminalChip): string | null => {
  const exit =
    chip.exitCode === undefined ? "" : chip.exitCode === null ? "still running" : `exit code ${chip.exitCode}`;
  if (chip.command === undefined) return exit ? `${exit[0]?.toUpperCase()}${exit.slice(1)}` : null;
  const command = chip.command.includes("\n")
    ? `Command:\n\`\`\`sh\n${fenceSafe(chip.command)}\n\`\`\``
    : `Command: ${inlineCode(chip.command)}`;
  return exit ? `${command}${chip.command.includes("\n") ? "\n" : " "}(${exit})` : command;
};

/** Where a file or a comment on it is: the thread's own checkout by its path, another repository's file under that repository's name. */
const where = (chip: { path: string; repoName?: string }): string => (chip.repoName ? `${chip.repoName}/${chip.path}` : chip.path);

export function chipLabel(chip: ComposerChip): string {
  switch (chip.kind) {
    case "file": {
      const at = where(chip);
      return chip.line === null ? at : `${at}:${chip.line}${chip.endLine && chip.endLine !== chip.line ? `–${chip.endLine}` : ""}`;
    }
    case "terminal":
      return chip.exitCode === undefined || chip.exitCode === null
        ? terminalLabel(chip)
        : `${terminalLabel(chip)} · exit ${chip.exitCode}`;
    case "review": {
      const at = where(chip);
      return chip.line === null ? at : `${at}:${chip.line}${chip.endLine && chip.endLine !== chip.line ? `–${chip.endLine}` : ""}`;
    }
    case "annotation":
      return chip.selector || chip.url;
    case "citation": {
      const first = oneLine(chip.text);
      return first.length > 48 ? `${first.slice(0, 48)}…` : first || "Cited reply";
    }
    case "thread":
      return chip.title;
    case "pull":
      return `#${chip.number} ${chip.title}`;
  }
}

/** What the engine receives for one chip. */
export function chipPrompt(chip: ComposerChip): string {
  switch (chip.kind) {
    case "file": {
      const at = where(chip);
      const atLine = chip.line === null ? `File \`${at}\`` : `File \`${at}:${chip.line}${chip.endLine && chip.endLine !== chip.line ? `-${chip.endLine}` : ""}\``;
      const body = chip.excerpt?.trim();
      if (!body) return atLine;
      const language = chip.language ?? "";
      return `${atLine}\n\`\`\`${language}\n${fenceSafe(excerptOf(body))}\n\`\`\``;
    }
    case "terminal": {
      const command = terminalCommand(chip);
      return `${terminalLabel(chip)}${command ? `\n${command}` : ""}\n\`\`\`\n${fenceSafe(excerptOf(chip.output))}\n\`\`\``;
    }
    case "review": {
      const heading = `Review on \`${chipLabel(chip)}\`${chip.stale ? " (the file was edited afterwards: this text may have changed or moved)" : ""}`;
      const excerpt = chip.excerpt?.trim()
        ? `\n\`\`\`\n${fenceSafe(excerptOf(chip.excerpt))}\n\`\`\``
        : "";
      return `${heading}\n${chip.comment.trim()}${excerpt}`;
    }
    case "annotation": {
      const lines = [`Element \`${chip.selector}\` on ${chip.url}`];
      if (chip.text.trim()) lines.push(quote(oneLine(chip.text)));
      if (chip.html.trim()) lines.push("```html", fenceSafe(excerptOf(chip.html)), "```");
      return lines.join("\n");
    }
    case "citation":
      return quote(excerptOf(chip.text.trim()));
    case "thread":
      // The link itself: the timeline renders it as a thread, and the agent
      // reads the thread on demand instead of the chip carrying its text.
      return `Thread: ${threadToken(chip.threadId, chip.title)}`;
    case "pull":
      // The link itself, with the number and title the pick carried.
      return `Pull request: ${pullToken(chip.number, chip.title, chip.url)}`;
  }
}

/** The prompt as sent: every chip, then what the person typed. */
export function promptWithChips(draft: string, chips: readonly ComposerChip[]): string {
  const blocks = chips.map(chipPrompt).filter((block) => block !== "");
  const typed = draft.trim();
  if (typed) blocks.push(typed);
  return blocks.join("\n\n");
}

/**
 * What a terminal selection becomes: its text without outer newlines, and the
 * one-based buffer rows it covered. Nothing is reread from a running terminal.
 */
export function terminalSelection(
  selected: string,
  position: { start: { y: number }; end: { y: number } } | null | undefined,
): { output: string; lineStart: number; lineEnd: number } | null {
  const output = selected.replace(/\r\n/g, "\n").replace(/^\n+/, "").replace(/\n+$/, "");
  if (!output.trim() || !position) return null;
  return {
    output,
    lineStart: Math.min(position.start.y, position.end.y) + 1,
    lineEnd: Math.max(position.start.y, position.end.y) + 1,
  };
}

const escapeHtml = (text: string) =>
  text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");

/**
 * The clipboard forms of a set of chips: JSON for Orbyn to restore, HTML so a
 * rich-text target keeps the labels, and plain prompt blocks for everything else.
 */
export function chipsClipboard(chips: readonly ComposerChip[]): { json: string; html: string; text: string } | null {
  // A chip that cannot be read back is not put on the clipboard as one.
  const valid = chips.filter((chip) => asChip(chip) !== null);
  if (valid.length === 0) return null;
  chips = valid;
  const payload = JSON.stringify({ version: 1, chips });
  const html = `<div ${CHIP_ATTRIBUTE}="${escapeHtml(payload)}"><ul>${chips
    .map((chip) => `<li>${escapeHtml(chipLabel(chip))}</li>`)
    .join("")}</ul></div>`;
  return { json: payload, html, text: chips.map(chipPrompt).join("\n\n") };
}

const text = (value: unknown, limit = CHIP_EXCERPT_LIMIT * 4): string | null =>
  typeof value === "string" && value.length <= limit ? value : null;

const number = (value: unknown): number | null =>
  typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null;

/** One half of a chip's repository identity: a non-blank string, or null when it is missing or the wrong shape. */
const repositoryField = (value: unknown): string | null => {
  const found = text(value, 256);
  return found !== null && found.trim() !== "" ? found : null;
};

/**
 * The repository a file or review chip names. Both halves name another repository of a multi-repository thread, whose
 * worktree holds the path; a name alone names the thread's primary repository, whose path and reads stay the thread's
 * own (the service's absent `repo_id` is the primary) and whose chip only says `<name>/<path>`. An id alone would read
 * another repository's file while the chip shows the thread's own path, so such a chip is refused.
 */
const repositoryOf = (chip: Record<string, unknown>): { repoId?: string; repoName?: string } | null => {
  const repoId = repositoryField(chip.repoId);
  const repoName = repositoryField(chip.repoName);
  if (repoId === null) return repoName === null ? {} : { repoName };
  return repoName === null ? null : { repoId, repoName };
};

/** One stored chip, or null when a field is missing or the wrong shape. */
export function asChip(value: unknown, id: string = chipId()): ComposerChip | null {
  if (typeof value !== "object" || value === null) return null;
  const chip = value as Record<string, unknown>;
  switch (chip.kind) {
    case "file": {
      const path = text(chip.path, 4096);
      if (!path) return null;
      const repo = repositoryOf(chip);
      if (repo === null) return null;
      return {
        id,
        kind: "file",
        path,
        line: number(chip.line),
        ...(number(chip.endLine) !== null ? { endLine: number(chip.endLine)! } : {}),
        ...(chip.fromDiff === true ? { fromDiff: true } : {}),
        ...repo,
        excerpt: text(chip.excerpt),
        language: text(chip.language, 64),
      };
    }
    case "terminal": {
      const output = text(chip.output);
      if (!output) return null;
      const lineStart = number(chip.lineStart);
      const lineEnd = number(chip.lineEnd);
      return {
        id,
        kind: "terminal",
        title: text(chip.title, 200) ?? "Terminal selection",
        output,
        lineStart,
        lineEnd: lineStart !== null && lineEnd !== null && lineEnd >= lineStart ? lineEnd : lineStart,
        ...(typeof chip.command === "string" && chip.command.length <= 4096 ? { command: chip.command } : {}),
        ...(chip.exitCode === null || (typeof chip.exitCode === "number" && Number.isInteger(chip.exitCode))
          ? { exitCode: chip.exitCode }
          : {}),
      };
    }
    case "review": {
      const path = text(chip.path, 4096);
      const comment = text(chip.comment, 20000);
      if (!path || comment === null) return null;
      const line = number(chip.line);
      const endLine = number(chip.endLine);
      if (chip.endLine !== undefined && (line === null || endLine === null || endLine < line)) return null;
      const repo = repositoryOf(chip);
      if (repo === null) return null;
      return {
        id, kind: "review", path, line, comment, excerpt: text(chip.excerpt),
        ...(endLine === null ? {} : { endLine }),
        ...repo,
        ...(typeof chip.sourceRevision === "string" ? { sourceRevision: text(chip.sourceRevision, 256) ?? undefined } : {}),
        ...(typeof chip.sourceCwd === "string" ? { sourceCwd: text(chip.sourceCwd, 4096) ?? undefined } : {}),
        ...(chip.stale === true ? { stale: true } : {}),
      };
    }
    case "annotation": {
      const url = text(chip.url, 8192);
      const selector = text(chip.selector, 4096);
      if (!url || !selector) return null;
      return {
        id,
        kind: "annotation",
        url,
        selector,
        text: text(chip.text) ?? "",
        html: text(chip.html) ?? "",
      };
    }
    case "citation": {
      const cited = text(chip.text);
      const threadId = text(chip.threadId, 200);
      if (!cited?.trim() || !threadId) return null;
      return { id, kind: "citation", threadId, text: cited };
    }
    case "thread": {
      const threadId = text(chip.threadId, 200);
      const title = text(chip.title, 4096);
      if (!threadId || !title) return null;
      return { id, kind: "thread", threadId, title };
    }
    case "pull": {
      const pullNumber = number(chip.number);
      const title = text(chip.title, 4096);
      const url = text(chip.url, 8192);
      if (pullNumber === null || !title || !url) return null;
      return { id, kind: "pull", number: pullNumber, title, url };
    }
    default:
      return null;
  }
}

const chipsFromJson = (raw: string): ComposerChip[] | null => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const group = parsed as { version?: unknown; chips?: unknown };
  if (group.version !== 1 || !Array.isArray(group.chips)) return null;
  const chips = group.chips.map((chip) => asChip(chip)).filter((chip): chip is ComposerChip => chip !== null);
  return chips.length > 0 ? chips : null;
};

/** The payload of the one element a chip copy left in HTML, if it is there. */
const htmlPayload = (html: string): string | null => {
  const match = new RegExp(`${CHIP_ATTRIBUTE}="([^"]*)"`).exec(html);
  if (!match?.[1]) return null;
  return match[1]
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
};

/**
 * Chips a paste carries, from Orbyn's own MIME type or its HTML form. Plain text
 * never becomes chips, so pasting JSON or Markdown is still just text.
 */
export function chipsFromClipboard(copy: {
  json?: string | null;
  html?: string | null;
}): ComposerChip[] | null {
  const fromJson = copy.json ? chipsFromJson(copy.json) : null;
  if (fromJson) return fromJson;
  const payload = copy.html ? htmlPayload(copy.html) : null;
  return payload ? chipsFromJson(payload) : null;
}

const LANGUAGE_BY_EXTENSION: Record<string, string> = {
  bash: "bash",
  c: "c",
  cc: "cpp",
  cpp: "cpp",
  css: "css",
  go: "go",
  h: "c",
  hpp: "cpp",
  html: "html",
  java: "java",
  js: "javascript",
  json: "json",
  jsx: "javascript",
  md: "markdown",
  mjs: "javascript",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sh: "sh",
  sql: "sql",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  yaml: "yaml",
  yml: "yaml",
};

/** The fence language a file's name suggests, or null when it names none. */
export function languageForPath(path: string): string | null {
  const name = path.split(/[\\/]/).at(-1) ?? path;
  const extension = name.includes(".") ? name.split(".").at(-1)?.toLowerCase() : null;
  return extension ? (LANGUAGE_BY_EXTENSION[extension] ?? null) : null;
}

/**
 * A file's text, trimmed around the line the person came from. A whole file is
 * bounded by the chip's own limit, so a large one never floods the prompt.
 */
export function fileExcerpt(text: string, line: number | null, contextLines = 40): string {
  if (line === null || line <= 0) return excerptOf(text);
  const lines = text.split("\n");
  const from = Math.max(line - contextLines - 1, 0);
  const to = Math.min(line + contextLines, lines.length);
  return excerptOf(lines.slice(from, to).join("\n"));
}

/** Files whose text the chip does not have yet, so a paste can refetch them. */
export const needsExcerpt = (chip: ComposerChip): chip is FileChip =>
  chip.kind === "file" && (chip.excerpt === null || chip.excerpt.trim() === "");

/**
 * Reads the files a pasted chip named but did not carry. The chip says which repository holds its file, so a paste reads
 * that repository's file and never the thread's own copy of the same path. A file that cannot be read keeps the chip and
 * its path; the prompt then says only where it was.
 */
export async function refillExcerpts(
  chips: readonly ComposerChip[],
  read: (chip: FileChip) => Promise<{ text: string; language?: string | null }>,
): Promise<ComposerChip[]> {
  const filled = await Promise.all(
    chips.map(async (chip) => {
      if (!needsExcerpt(chip)) return chip;
      try {
        const found = await read(chip);
        return {
          ...chip,
          excerpt: excerptOf(found.text),
          language: found.language ?? chip.language,
        };
      } catch {
        return chip;
      }
    }),
  );
  return filled;
}
