import { ChevronDown, ChevronRight, ChevronUp, Folder, Hash, Lock, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Button, IconButton } from "../../../ui/controls";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { CommentForm, CommentList, type LineSelection } from "../files/FileComments";
import { CodeEditor, type CodeEditorHandle } from "./CodeEditor";
import { eolOf, toLF, findMatches, indentOf, languageOf, lineCount, lineStart } from "./editorModel";

const isMac = () => /mac/i.test(navigator.platform || navigator.userAgent);

export type Standing = "saved" | "unsaved" | "read-only";

/**
 * The editor drawn in Engineer Designs v106 (editor-default and its states): the folders and name of the file with
 * Find and Go to line, a banner slot, the code, and the bar that says where the cursor is and whether the file is saved.
 * The text and what happens to it belong to the caller; a read-only caller passes no `onChange`.
 */
export function EditorSurface({
  path,
  text: raw,
  eol,
  standing,
  banner,
  reveal = null,
  comments,
  onChange,
  onSave,
}: {
  path: string;
  text: string;
  /** The file's line ending, when `text` has already had its breaks made line feeds. */
  eol?: "LF" | "CRLF";
  standing: Standing;
  banner?: ReactNode;
  /** A line a link named: the cursor goes there once, when the file opens. */
  reveal?: number | null;
  /** Where the file lives, so a line comment can name it: the comments become chips on the thread's next message. */
  comments?: { threadId: string; cwd: string; revision: string; text: string };
  onChange?(text: string): void;
  onSave?(): void;
}) {
  const text = toLF(raw);
  const root = useRef<HTMLDivElement>(null);
  const editor = useRef<CodeEditorHandle>(null);
  const [bar, setBar] = useState<"find" | "goto" | null>(null);
  const [needle, setNeedle] = useState("");
  const [at, setAt] = useState(0);
  const [lineText, setLineText] = useState("");
  const [position, setPosition] = useState({ line: 1, col: 1 });
  const offset = useRef(0);
  const [selection, setSelection] = useState<LineSelection | null>(null);
  const chips = useComposerChips((state) => (comments ? state.chips[comments.threadId] : undefined));
  const field = useRef<HTMLInputElement>(null);
  const folders = path.split("/").slice(0, -1);
  const name = path.split("/").at(-1) ?? path;
  const language = languageOf(path);
  const total = lineCount(text);
  const matches = useMemo(() => (bar === "find" ? findMatches(text, needle) : []), [bar, text, needle]);
  const shown = matches.length === 0 ? -1 : Math.min(at, matches.length - 1);

  useEffect(() => {
    if (bar) field.current?.focus();
  }, [bar]);
  const revealed = useRef<number | null>(null);
  useEffect(() => {
    if (reveal === null || revealed.current === reveal) return;
    revealed.current = reveal;
    const start = lineStart(text, Math.min(Math.max(reveal, 1), total));
    editor.current?.select(start, start);
  }, [reveal, text, total]);

  const jump = (index: number) => {
    const match = matches[index];
    if (!match) return;
    setAt(index);
    editor.current?.select(match.start, match.end);
  };
  const step = (direction: 1 | -1) => {
    if (matches.length === 0) return;
    jump((shown + direction + matches.length) % matches.length);
  };
  const changeNeedle = (value: string) => {
    setNeedle(value);
    const next = findMatches(text, value);
    // The first match at or after the cursor is the current one.
    const first = Math.max(0, next.findIndex((match) => match.start >= offset.current));
    setAt(first);
    const match = next[first];
    if (match) editor.current?.select(match.start, match.end);
  };
  const go = () => {
    const wanted = Number.parseInt(lineText, 10);
    if (!Number.isFinite(wanted)) return;
    // A number outside the range goes to the nearest line, and the field shows it.
    const line = Math.min(Math.max(wanted, 1), total);
    setLineText(String(line));
    const start = lineStart(text, line);
    editor.current?.select(start, start);
    editor.current?.focus();
  };
  const closeBar = () => {
    setBar(null);
    editor.current?.focus();
  };
  const open = (which: "find" | "goto") => {
    if (bar === which) field.current?.focus();
    else setBar(which);
    if (which === "goto" && lineText === "") setLineText(String(position.line));
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const accel = isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
    if (!accel || event.altKey || event.shiftKey) return;
    const key = event.key.toLowerCase();
    if (key === "f" || key === "g") {
      event.preventDefault();
      event.stopPropagation();
      open(key === "f" ? "find" : "goto");
    } else if (key === "s" && onSave) {
      event.preventDefault();
      event.stopPropagation();
      onSave();
    }
  };
  const count = needle === "" ? "" : matches.length === 0 ? "No matches" : `${shown + 1} of ${matches.length}`;

  return (
    <div className="ed" ref={root} onKeyDown={onKeyDown}>
      <div className="ed-crumb">
        <Folder size={14} aria-hidden />
        {folders.map((part, index) => (
          <span key={index} className="ed-part">
            {part}
            <ChevronRight size={12} aria-hidden />
          </span>
        ))}
        <b title={path}>{name}</b>
        <span className="ed-sp" />
        <IconButton label="Find ⌘F" pressed={bar === "find"} onClick={() => (bar === "find" ? closeBar() : open("find"))}>
          <Search size={14} aria-hidden />
        </IconButton>
        <IconButton label="Go to line ⌘G" pressed={bar === "goto"} onClick={() => (bar === "goto" ? closeBar() : open("goto"))}>
          <Hash size={14} aria-hidden />
        </IconButton>
      </div>
      {bar === "find" ? (
        <div className="ed-bar" role="search" aria-label="Find in file">
          <label className="ed-in">
            <Search size={14} aria-hidden />
            <input
              ref={field}
              aria-label="Find in file"
              value={needle}
              onChange={(event) => changeNeedle(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  step(event.shiftKey ? -1 : 1);
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  closeBar();
                }
              }}
            />
          </label>
          <span className="ed-cnt" aria-live="polite">
            {count}
          </span>
          <IconButton label="Previous match" onClick={() => step(-1)}>
            <ChevronUp size={14} aria-hidden />
          </IconButton>
          <IconButton label="Next match" onClick={() => step(1)}>
            <ChevronDown size={14} aria-hidden />
          </IconButton>
          <IconButton label="Close find" onClick={closeBar}>
            <X size={14} aria-hidden />
          </IconButton>
        </div>
      ) : null}
      {bar === "goto" ? (
        <div className="ed-bar" role="group" aria-label="Go to line">
          <span className="ed-lab">Go to line</span>
          <label className="ed-in">
            <input
              ref={field}
              aria-label="Line number"
              inputMode="numeric"
              value={lineText}
              onChange={(event) => setLineText(event.target.value.replace(/\D/g, ""))}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  go();
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  closeBar();
                }
              }}
            />
          </label>
          <span className="ed-cnt">1 to {total}</span>
          <Button small tone="quiet" onClick={go}>
            Go
          </Button>
        </div>
      ) : null}
      {banner}
      <CodeEditor
        ref={editor}
        text={text}
        label={`${standing === "read-only" ? "Read" : "Edit"} ${path}`}
        grammar={language.grammar}
        readOnly={standing === "read-only" || !onChange}
        matches={matches}
        current={shown}
        {...(comments
          ? {
              onComment: (line: number, extend: boolean) => setSelection((now) => ({ anchor: extend && now ? now.anchor : line, end: line })),
              selected: selection ? { from: Math.min(selection.anchor, selection.end), to: Math.max(selection.anchor, selection.end) } : null,
              commented: (chips ?? []).flatMap((chip) => (chip.kind === "review" && chip.path === path && chip.sourceRevision === comments.revision && chip.sourceCwd === comments.cwd && chip.line ? [chip.line] : [])),
            }
          : {})}
        onChange={(next) => onChange?.(next)}
        onCursor={(next, where) => {
          setPosition(next);
          offset.current = where;
        }}
      />
      {comments ? (
        <div className="ed-comments">
          {selection ? <CommentForm {...{ path, ...comments }} selection={selection} onDone={() => setSelection(null)} /> : null}
          <CommentList {...{ path, ...comments }} />
        </div>
      ) : null}
      <div className="ed-status">
        <span>
          Ln {position.line}, Col {position.col}
        </span>
        <span className="ed-meta">
          {language.label} · UTF-8 · {eol ?? eolOf(raw)} · {indentOf(text)}
        </span>
        <span className="ed-end" data-standing={standing}>
          {standing === "read-only" ? <Lock size={12} aria-hidden /> : null}
          {standing === "saved" ? "Saved" : standing === "unsaved" ? "Unsaved" : "Read-only"}
        </span>
      </div>
    </div>
  );
}
