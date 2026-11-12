import { Fragment, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useWorkspace } from "../../../product/state/workspace";
import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { charactersOver, chipId, chipLabel, promptWithChips, type ReviewChip } from "../../../product/composer/composerContext";
import { Button } from "../../../ui/controls";
import { LINE_LIMIT, LOCK_COLLAPSE, diffCommentRange, gutterNumber, hunkLabel, markers, numberRows, splitRows, type DiffLine, type Row } from "./diffModel";

const count = (value: number) => value.toLocaleString("en-US");

/** A line's text with every find match marked; the current one says so. */
function Text({ text, needle, currentAt }: { text: string; needle: string; currentAt: number | null }) {
  if (needle === "") return <>{text}</>;
  const lower = text.toLowerCase();
  const parts: ReactNode[] = [];
  let from = 0;
  for (let at = lower.indexOf(needle); at >= 0; at = lower.indexOf(needle, from)) {
    if (at > from) parts.push(text.slice(from, at));
    parts.push(
      <mark key={at} className="fhit" aria-current={at === currentAt || undefined}>
        {text.slice(at, at + needle.length)}
      </mark>,
    );
    from = at + needle.length;
  }
  parts.push(text.slice(from));
  return <>{parts}</>;
}

interface Pick {
  anchor: number;
  end: number;
}

/**
 * A diff comment is a review chip: the composer owns it, and the patch shows it under the line it ends
 * on for as long as the chip exists. Removing either removes both. `origin` ties it to this diff view,
 * as a file comment is tied to the source it was made on.
 */
export function Patch({ lines, path, threadId, origin, needle, current, lock, full, split, onShowAll }: {
  lines: readonly DiffLine[];
  path: string;
  threadId: string;
  origin: string;
  /** The lower-cased find text, or "" when nothing is searched. */
  needle: string;
  /** The row and start of the match the find bar is on in this file. */
  current: { row: number; start: number } | null;
  /** A lock file starts with none of its lines shown. */
  lock: boolean;
  /** Show anyway was pressed: every line is drawn. */
  full: boolean;
  /** The panel is wide enough for Split; below that it stays Unified whatever is chosen. */
  split: boolean;
  onShowAll(): void;
}) {
  const storedLayout = useUi((state) => state.diffLayout);
  const layout = split ? storedLayout : "stacked";
  const wrap = useUi((state) => state.diffWrapLines);
  const pushToast = useUi((state) => state.pushToast);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const chips = useComposerChips((state) => state.chips[threadId]);
  const rows = useMemo(() => numberRows(lines), [lines]);
  const limit = full ? rows.length : lock && rows.length > LOCK_COLLAPSE ? 0 : LINE_LIMIT;
  const shown = rows.length > limit ? rows.slice(0, limit) : rows;
  const hidden = rows.slice(shown.length).filter((row) => row.line.kind === "add" || row.line.kind === "del").length;
  const [pick, setPick] = useState<Pick | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const dragging = useRef(false);
  const dragged = useRef(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!current) return;
    root.current?.querySelector(`[data-row="${current.row}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [current]);
  useEffect(() => {
    const stop = () => { dragging.current = false; };
    window.addEventListener("pointerup", stop);
    return () => window.removeEventListener("pointerup", stop);
  }, []);

  const comments = (chips ?? []).filter((chip): chip is ReviewChip => chip.kind === "review" && chip.path === path
    && chip.sourceRevision === origin && (chip.sourceCwd ?? "") === (cwd ?? ""));
  const low = pick ? Math.min(pick.anchor, pick.end) : -1;
  const high = pick ? Math.max(pick.anchor, pick.end) : -1;
  const range = pick ? diffCommentRange(rows, pick.anchor, pick.end) : null;
  const where = range ? `${path} line ${range.line}${range.endLine !== range.line ? `–${range.endLine}` : ""}` : path;
  const heading = range
    ? range.endLine !== range.line ? `Comment on lines ${range.line}–${range.endLine}` : `Comment on line ${range.line}`
    : "Comment";
  const cancel = () => { setPick(null); setText(""); setError(null); };
  const start = (at: number, extend: boolean) => {
    setPick((now) => ({ anchor: extend && now ? now.anchor : at, end: at }));
    setCursor(at);
    setError(null);
  };
  const add = () => {
    if (!range || text.trim() === "") return;
    const chip: ReviewChip = {
      id: chipId(), kind: "review", path, ...range, comment: text.trim(), sourceRevision: origin,
      ...(cwd ? { sourceCwd: cwd } : {}),
    };
    if (chip.endLine === chip.line) delete chip.endLine;
    const now = useComposerChips.getState().chips[threadId] ?? [];
    if (charactersOver(promptWithChips(useUi.getState().drafts[threadId] ?? "", [...now, chip]))) {
      setError("This comment would make your message too long. Shorten it before adding it.");
      return;
    }
    useComposerChips.getState().add(threadId, chip);
    pushToast("Review comment added to your message");
    cancel();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.target !== event.currentTarget) return;
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step) {
      event.preventDefault();
      const at = Math.min(Math.max((cursor ?? (step > 0 ? -1 : shown.length)) + step, 0), shown.length - 1);
      if (event.shiftKey && shown[at]?.newNo != null) {
        setPick((now) => ({ anchor: now?.anchor ?? cursor ?? at, end: at }));
        setCursor(at);
        setError(null);
      } else setCursor(at);
    } else if ((event.key === "c" || event.key === "C") && !event.metaKey && !event.ctrlKey && !event.altKey && cursor !== null && shown[cursor]?.newNo != null) {
      event.preventDefault();
      start(cursor, false);
    }
  };

  const gutter = (row: Row, side: "old" | "new") => {
    const number = side === "old" ? row.oldNo : row.newNo;
    const commentable = side === "new" && row.newNo !== null;
    if (!commentable) return <i>{number}</i>;
    return (
      <button
        type="button"
        className="gut"
        tabIndex={-1}
        aria-label={`Comment on line ${row.newNo}`}
        aria-pressed={pick !== null && row.at >= low && row.at <= high}
        onPointerDown={(event) => {
          if (event.button !== 0 || event.shiftKey) return;
          dragging.current = true;
          dragged.current = false;
          setPick({ anchor: row.at, end: row.at });
          setCursor(row.at);
        }}
        onClick={(event) => {
          if (dragged.current) { dragged.current = false; return; }
          start(row.at, event.shiftKey);
        }}
      >
        <span className="gn">{number}</span>
        <span className="gp" aria-hidden>+</span>
      </button>
    );
  };

  const cell = (row: Row | null, side: "old" | "new", split: boolean) => {
    if (!row) return <div className="l ghost-cell" aria-hidden />;
    const kind = row.line.kind;
    const selected = pick !== null && row.at >= low && row.at <= high;
    const mark = markers[kind];
    const body = row.line.text.slice(1);
    return (
      <div
        className={`l ${kind === "add" ? "add" : kind === "del" ? "del" : ""} ${selected ? "lsel" : ""} ${cursor === row.at && focused ? "hov" : ""}`}
        data-row={row.at}
        onPointerEnter={() => {
          if (!dragging.current || row.newNo === null && row.line.kind !== "del") return;
          dragged.current = true;
          setPick((now) => (now ? { ...now, end: row.at } : now));
        }}
      >
        {split || side === "new" ? gutter(row, side) : <i>{gutterNumber(row)}</i>}
        <b aria-hidden>{kind === "add" ? "+" : kind === "del" ? "−" : ""}</b>
        <code>
          {mark ? <span className="sr-only">{mark}: </span> : null}
          <Text text={body || " "} needle={needle} currentAt={current?.row === row.at ? current.start : null} />
        </code>
      </div>
    );
  };

  const unifiedCell = (row: Row) => {
    const kind = row.line.kind;
    if (kind === "hunk") return <div key={row.at} className="hh2" data-row={row.at}>{hunkLabel(row.line.text)}</div>;
    if (kind === "meta") return <div key={row.at} className="hh2" data-row={row.at}>{row.line.text}</div>;
    // A removed line numbers on the old side; the rest on the new side, where a comment can be made.
    return cell(row, kind === "del" ? "old" : "new", false);
  };

  const after = (row: Row | null) => {
    if (!row || row.newNo === null) return null;
    const here = comments.filter((chip) => (chip.endLine ?? chip.line) === row.newNo);
    const form = pick !== null && row.at === high;
    if (here.length === 0 && !form) return null;
    return (
      <>
        {here.map((chip) => (
          <div key={chip.id} className="cmt2" role="note" aria-label={`Review comment ${chipLabel(chip)}`}>
            <span>
              <b>You</b> · {chip.comment} <span className="t3">Sent to the agent</span>
            </span>
            <div className="row">
              <Button small tone="quiet" onClick={() => useComposerChips.getState().remove(threadId, chip.id)}>
                Remove comment
              </Button>
            </div>
          </div>
        ))}
        {form ? (
          <form
            className="cmt2"
            onSubmit={(event) => { event.preventDefault(); add(); }}
            onKeyDown={(event) => {
              if (event.key === "Escape") { event.stopPropagation(); cancel(); }
              else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) { event.preventDefault(); add(); }
            }}
          >
            <label htmlFor={`diff-comment-${path}`} className="t3">{heading}</label>
            <textarea
              id={`diff-comment-${path}`}
              className="ta"
              aria-label={`Review comment on ${where}`}
              value={text}
              maxLength={20000}
              autoFocus
              onChange={(event) => setText(event.target.value)}
            />
            {!range ? <p role="alert" className="t3">Select at most 4,000 characters of the diff for a comment.</p> : null}
            {error ? <p role="alert" className="t3">{error}</p> : null}
            <div className="row">
              <span className="t3 grow">Sent to the agent</span>
              <Button small tone="quiet" onClick={cancel}>Cancel</Button>
              <Button small tone="primary" type="submit" disabled={!range || text.trim() === ""}>Comment</Button>
            </div>
          </form>
        ) : null}
      </>
    );
  };

  const body = layout === "split"
    ? splitRows(shown).map((pair, at) => (
        <Fragment key={at}>
          {pair.left && (pair.left.line.kind === "hunk" || pair.left.line.kind === "meta") ? (
            unifiedCell(pair.left)
          ) : (
            <div className="sp2">
              {cell(pair.left, "old", true)}
              {cell(pair.right, "new", true)}
            </div>
          )}
          {after(pair.right)}
        </Fragment>
      ))
    : shown.map((row) => (
        <Fragment key={row.at}>
          {unifiedCell(row)}
          {after(row)}
        </Fragment>
      ));

  return (
    <>
      <div
        ref={root}
        className="d2"
        tabIndex={0}
        role="group"
        aria-label="Patch"
        data-layout={layout}
        data-wrap={wrap ? "on" : "off"}
        onKeyDown={onKeyDown}
        onFocus={(event) => { if (event.target === event.currentTarget) setFocused(true); }}
        onBlur={(event) => { if (event.target === event.currentTarget) setFocused(false); }}
      >
        {body}
      </div>
      {rows.length > shown.length ? (
        <div className="x2banner">
          <span>Large diff: {count(hidden)} lines not shown.</span>
          <span className="sp" />
          <Button small tone="quiet" onClick={onShowAll}>Show anyway</Button>
        </div>
      ) : null}
    </>
  );
}
