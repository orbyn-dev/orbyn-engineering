import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { highlight, type Token } from "../../content";
import { lineCount, piecesOf, positionOf } from "./editorModel";

/** Past this many lines the colour is off, so typing stays quick. */
const COLOUR_LINES = 20_000;
const LINE = 19;
const TOP = 4;

export interface CodeEditorHandle {
  /** Selects a range and scrolls its line into view, without taking focus. */
  select(start: number, end: number): void;
  focus(): void;
}

export interface Mark {
  start: number;
  end: number;
}

/**
 * The text of a file with line numbers, colour, the current line and, optionally, find marks. One textarea
 * takes the typing; the coloured copy sits under it and follows its scroll. Saving and reading are the caller's.
 */
export const CodeEditor = forwardRef<
  CodeEditorHandle,
  {
    text: string;
    label: string;
    grammar: string | null;
    readOnly: boolean;
    matches?: readonly Mark[];
    current?: number;
    /** Lines may be commented on: a "+" beside the line number under the pointer, or the cursor's line while the code has focus. */
    onComment?(line: number, extend: boolean): void;
    /** Why commenting is off for now; the + shows, disabled, with this as its hint. */
    commentBlocked?: string;
    /** The lines a comment is being written for, and the lines that already have one. */
    selected?: { from: number; to: number } | null;
    commented?: readonly number[];
    onChange(text: string): void;
    onCursor(position: { line: number; col: number }, offset: number): void;
  }
>(function CodeEditor({ text, label, grammar, readOnly, matches = [], current = -1, onComment, commentBlocked, selected = null, commented = [], onChange, onCursor }, ref) {
  const area = useRef<HTMLTextAreaElement>(null);
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const [cursor, setCursor] = useState(0);
  const [hover, setHover] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const lines = useMemo(() => text.split("\n"), [text]);
  const coloured = grammar !== null && lines.length <= COLOUR_LINES;
  // Tokens follow the text a moment late; a line whose text changed shows plain until they catch up.
  const [tokens, setTokens] = useState<Token[][] | null>(null);
  useEffect(() => {
    if (!coloured || !grammar) return;
    let live = true;
    const timer = setTimeout(() => {
      highlight(text, grammar).then(
        (result) => live && setTokens(result),
        () => live && setTokens(null),
      );
    }, 120);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [text, grammar, coloured]);

  const report = useCallback(
    (element: HTMLTextAreaElement) => {
      setCursor(element.selectionStart);
      onCursor(positionOf(element.value, element.selectionStart), element.selectionStart);
    },
    [onCursor],
  );
  useImperativeHandle(
    ref,
    () => ({
      focus: () => area.current?.focus(),
      select: (start, end) => {
        const element = area.current;
        if (!element) return;
        element.setSelectionRange(start, end);
        const row = positionOf(element.value, start).line - 1;
        const top = row * LINE;
        if (top < element.scrollTop || top + LINE * 2 > element.scrollTop + element.clientHeight) element.scrollTop = Math.max(0, top - element.clientHeight / 2);
        setScroll({ top: element.scrollTop, left: element.scrollLeft });
        report(element);
      },
    }),
    [report],
  );

  const offsets = useMemo(() => {
    const out: number[] = [];
    let at = 0;
    for (const line of lines) {
      out.push(at);
      at += line.length + 1;
    }
    return out;
  }, [lines]);
  // Each match belongs to the line it starts on (a find text holds no line break).
  const byLine = useMemo(() => {
    const out = new Map<number, { start: number; end: number; id: number }[]>();
    matches.forEach((match, id) => {
      let low = 0;
      let high = offsets.length - 1;
      while (low < high) {
        const mid = (low + high + 1) >> 1;
        if ((offsets[mid] ?? 0) <= match.start) low = mid;
        else high = mid - 1;
      }
      out.set(low, [...(out.get(low) ?? []), { ...match, id }]);
    });
    return out;
  }, [matches, offsets]);
  const row = positionOf(text, cursor).line;
  const total = lineCount(text);

  return (
    <div
      className="ed-body"
      onFocusCapture={() => setFocused(true)}
      onBlurCapture={(event) => {
        // Focus moving to the + (or back) stays inside the editor.
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      <div
        className="ed-gutw"
        onMouseMove={(event) => {
          // Over the + itself the line stays the one it marks.
          if (!onComment || (event.target as HTMLElement).closest(".ed-plus")) return;
          const box = event.currentTarget.getBoundingClientRect();
          setHover(Math.min(Math.max(1, Math.floor((event.clientY - box.top - TOP + scroll.top) / LINE) + 1), total));
        }}
        onMouseLeave={() => setHover(null)}
      >
        <pre className="ed-gut" aria-hidden>
          <span style={{ transform: `translateY(${-scroll.top}px)` }}>{lines.map((_, at) => at + 1).join("\n")}</span>
        </pre>
        {commented.map((line) => (
          <i key={line} className="ed-dot" aria-hidden style={{ transform: `translateY(${TOP + (line - 1) * LINE - scroll.top}px)` }} />
        ))}
        {onComment && (hover ?? (focused ? row : null)) !== null ? (
          <button
            type="button"
            className="ed-plus"
            aria-label={`Comment on line ${hover ?? row}`}
            disabled={commentBlocked !== undefined}
            title={commentBlocked}
            style={{ transform: `translateY(${TOP + ((hover ?? row) - 1) * LINE - scroll.top}px)` }}
            onClick={(event) => onComment(hover ?? row, event.shiftKey)}
          >
            +
          </button>
        ) : null}
      </div>
      <div className="ed-main">
        {selected ? (
          <div className="ed-sel" aria-hidden style={{ transform: `translateY(${TOP + (selected.from - 1) * LINE - scroll.top}px)`, height: (selected.to - selected.from + 1) * LINE }} />
        ) : null}
        <div className="ed-cur" aria-hidden style={{ transform: `translateY(${TOP + (row - 1) * LINE - scroll.top}px)` }} />
        <pre className="ed-hl" aria-hidden>
          <span style={{ transform: `translate(${-scroll.left}px, ${-scroll.top}px)` }}>
            {lines.map((line, at) => {
              const from = offsets[at] ?? 0;
              const own = byLine.get(at) ?? [];
              const colours = coloured ? tokens?.[at] : undefined;
              const fresh = colours && colours.map((token) => token.content).join("") === line ? colours : null;
              const pieces = piecesOf(fresh ?? [{ content: line }], from, own);
              return (
                <span key={at}>
                  {pieces.map((piece, index) => (
                    <span
                      key={index}
                      style={piece.color ? { color: piece.color } : undefined}
                      data-mark={piece.match === undefined ? undefined : piece.match === current ? "current" : "match"}
                    >
                      {piece.text}
                    </span>
                  ))}
                  {"\n"}
                </span>
              );
            })}
          </span>
        </pre>
        <textarea
          ref={area}
          className="ed-ta"
          aria-label={label}
          aria-multiline="true"
          wrap="off"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          readOnly={readOnly}
          value={text}
          data-lines={total}
          onChange={(event) => {
            onChange(event.target.value);
            report(event.target);
          }}
          onScroll={(event) => setScroll({ top: event.currentTarget.scrollTop, left: event.currentTarget.scrollLeft })}
          onSelect={(event) => report(event.currentTarget)}
          onKeyUp={(event) => report(event.currentTarget)}
          onClick={(event) => report(event.currentTarget)}
        />
      </div>
    </div>
  );
});
