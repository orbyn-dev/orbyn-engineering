import { ArrowUp, Copy, MessageSquare, MoreHorizontal, SquarePen, Undo2 } from "lucide-react";
import { Fragment, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useWorkspace } from "../../../product/state/workspace";
import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { charactersOver, chipId, chipLabel, languageForPath, promptWithChips, type ComposerChip, type ReviewChip } from "../../../product/composer/composerContext";
import { ConfirmDialog } from "../../../ui/Dialog";
import { Button, IconButton } from "../../../ui/controls";
import { diffCommentRange, gutterNumber, hunkLabel, markers, numberRows, shownLimit, splitRows, type DiffLine, type Row } from "./diffModel";
import { canRevert, changedLines, hunkPatch, hunksOf, type Hunk } from "./hunkModel";
import { MenuButton } from "./ChangesHeader";

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

/** What a hunk or a line of this file can do to the working tree; absent where the diff is only to read. */
export interface PatchActions {
  /** False for a file with no earlier version to go back to (a new file). */
  revertible: boolean;
  revert(hunk: Hunk): Promise<void>;
  /** Writes line `number` (new side) from `was` to `next`; true when it was written. */
  saveLine(number: number, was: string, next: string): Promise<boolean>;
}

/**
 * A diff comment is a review chip: the composer owns it, and the patch shows it under the line it ends
 * on for as long as the chip exists. Removing either removes both. `origin` ties it to this diff view,
 * as a file comment is tied to the source it was made on.
 */
export function Patch({ lines, path, threadId, origin, needle, current, full, split, onShowAll, actions, narrow }: {
  lines: readonly DiffLine[];
  path: string;
  threadId: string;
  origin: string;
  /** The lower-cased find text, or "" when nothing is searched. */
  needle: string;
  /** The row and start of the match the find bar is on in this file. */
  current: { row: number; start: number } | null;
  /** Show anyway was pressed: every line is drawn. */
  full: boolean;
  /** The panel is wide enough for Split; below that it stays Unified whatever is chosen. */
  split: boolean;
  onShowAll(): void;
  actions?: PatchActions;
  /** The panel is too narrow for a hunk's buttons: they fold into one More menu. */
  narrow?: boolean;
}) {
  const storedLayout = useUi((state) => state.diffLayout);
  const layout = split ? storedLayout : "stacked";
  const wrap = useUi((state) => state.diffWrapLines);
  const pushToast = useUi((state) => state.pushToast);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const chips = useComposerChips((state) => state.chips[threadId]);
  const rows = useMemo(() => numberRows(lines), [lines]);
  const shown = rows.slice(0, shownLimit(rows.length, path, full));
  const hidden = rows.slice(shown.length).filter((row) => row.line.kind === "add" || row.line.kind === "del").length;
  const [pick, setPick] = useState<Pick | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState("");
  const [commenting, setCommenting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [overHunk, setOverHunk] = useState<number | null>(null);
  const [confirm, setConfirm] = useState<Hunk | null>(null);
  const [editing, setEditing] = useState<{ at: number; text: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const dragging = useRef(false);
  const dragged = useRef(false);
  const lastRow = useRef<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const hintId = useId();
  const hunks = useMemo(() => hunksOf(rows), [rows]);
  const hunkOf = (at: number) => {
    let found: Hunk | undefined;
    for (const hunk of hunks) if (hunk.at <= at) found = hunk;
    return found;
  };

  useEffect(() => {
    if (!current) return;
    root.current?.querySelector(`[data-row="${current.row}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [current]);
  useEffect(() => {
    // The arrow keys carry focus with the cursor to that line's button, so a screen reader announces the line.
    const element = root.current;
    if (cursor === null || !element?.contains(document.activeElement)) return;
    if (document.activeElement instanceof HTMLTextAreaElement || document.activeElement instanceof HTMLInputElement) return;
    element.querySelector<HTMLElement>(`[data-row="${cursor}"] .gut`)?.focus();
  }, [cursor]);
  useEffect(() => {
    const stop = () => { dragging.current = false; };
    window.addEventListener("pointerup", stop);
    return () => window.removeEventListener("pointerup", stop);
  }, []);

  const sentChips = useComposerChips((state) => state.sent[threadId]);
  const here = (chip: ComposerChip): chip is ReviewChip => chip.kind === "review" && chip.path === path
    && chip.sourceRevision === origin && (chip.sourceCwd ?? "") === (cwd ?? "");
  const pending = (chips ?? []).filter(here);
  const sent = (sentChips ?? []).filter(here);
  const comments = [...sent, ...pending];
  const low = pick ? Math.min(pick.anchor, pick.end) : -1;
  const high = pick ? Math.max(pick.anchor, pick.end) : -1;
  const range = pick ? diffCommentRange(rows, pick.anchor, pick.end) : null;
  const where = range ? `${path} line ${range.line}${range.endLine !== range.line ? `–${range.endLine}` : ""}` : path;
  const heading = range
    ? range.endLine !== range.line ? `Comment on lines ${range.line}–${range.endLine}` : `Comment on line ${range.line}`
    : "Comment";
  const cancel = () => { setPick(null); setText(""); setError(null); setCommenting(false); };
  const start = (at: number, extend: boolean) => {
    setPick((now) => ({ anchor: extend && now ? now.anchor : at, end: at }));
    setCursor(at);
    setError(null);
    // A click comments on its line; Shift and a drag select first, and the bar under the selection offers Comment.
    setCommenting(!extend);
  };
  const editable = (row: Row | null): row is Row => actions !== undefined && row !== null && row.newNo !== null
    && (row.line.kind === "add" || row.line.kind === "ctx");
  const edit = (row: Row) => { cancel(); setEditing({ at: row.at, text: row.line.text.slice(1).replace(/\r$/, "") }); };
  const save = async (row: Row) => {
    if (!actions || !editing || saving || row.newNo === null) return;
    const was = row.line.text.slice(1).replace(/\r$/, "");
    if (editing.text === was) { setEditing(null); return; }
    setSaving(true);
    const written = await actions.saveLine(row.newNo, was, editing.text);
    setSaving(false);
    if (written) setEditing(null);
  };
  /** The selected lines go to the composer as one chip; nothing is sent until the person sends the message. */
  const send = () => {
    if (!range) return;
    const have = useComposerChips.getState().chips[threadId] ?? [];
    if (have.some((chip) => chip.kind === "file" && chip.path === path && chip.line === range.line && (chip.endLine ?? range.line) === range.endLine)) return;
    const chip: ComposerChip = {
      id: chipId(), kind: "file", path, line: range.line, fromDiff: true, excerpt: range.excerpt, language: languageForPath(path),
      ...(range.endLine !== range.line ? { endLine: range.endLine } : {}),
    };
    if (charactersOver(promptWithChips(useUi.getState().drafts[threadId] ?? "", [...have, chip]))) {
      setError("This selection would make your message too long. Shorten it before adding it.");
      return;
    }
    useComposerChips.getState().add(threadId, chip);
  };
  const copySelection = () => {
    if (range) void navigator.clipboard?.writeText(range.excerpt).then(() => pushToast("Lines copied"));
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
    // Typing in the comment box or a line being edited keeps its keys.
    const target = event.target as HTMLElement;
    if (target.closest("form, textarea, input")) return;
    // Enter on a button of the selection bar or a hunk header presses that button; only the gutter and the diff itself take it as a command.
    if (event.key === "Enter" && target.closest("button") && !target.classList.contains("gut")) return;
    if (event.key === "Escape" && pick) { event.stopPropagation(); cancel(); return; }
    // Enter on a selection sends it to the engine; on a single line it still opens the comment.
    if (event.key === "Enter" && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
      if (pick && !commenting && range) { event.preventDefault(); send(); return; }
    }
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step) {
      event.preventDefault();
      const at = Math.min(Math.max((cursor ?? (step > 0 ? -1 : shown.length)) + step, 0), shown.length - 1);
      if (event.shiftKey && shown[at]?.newNo != null) {
        setPick((now) => ({ anchor: now?.anchor ?? cursor ?? at, end: at }));
        setCursor(at);
        setError(null);
        setCommenting(false);
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
        tabIndex={cursor === row.at ? 0 : -1}
        aria-label={`Comment on line ${row.newNo}`}
        aria-pressed={pick !== null && row.at >= low && row.at <= high}
        onPointerDown={(event) => {
          if (event.button !== 0 || event.shiftKey) return;
          dragging.current = true;
          dragged.current = false;
          setPick({ anchor: row.at, end: row.at });
          setCursor(row.at);
          setCommenting(false);
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
    if (editing?.at === row.at && side === "new") {
      return (
        <div className="pd-ledit" data-row={row.at}>
          <i>{row.newNo}</i>
          <b aria-hidden>{kind === "add" ? "+" : ""}</b>
          <input
            className="pd-field"
            aria-label={`Edit line ${row.newNo}`}
            value={editing.text}
            autoFocus
            spellCheck={false}
            onChange={(event) => setEditing({ at: row.at, text: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === "Escape") { event.stopPropagation(); setEditing(null); }
              else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) { event.preventDefault(); void save(row); }
            }}
          />
        </div>
      );
    }
    return (
      <div
        className={`l ${kind === "add" ? "add" : kind === "del" ? "del" : ""} ${selected ? "lsel" : ""} ${cursor === row.at && focused ? "hov" : ""}`}
        data-row={row.at}
        onDoubleClick={editable(row) && side === "new" ? () => edit(row) : undefined}
        onPointerEnter={() => {
          lastRow.current = row.at;
          if (hunkOf(row.at)) setOverHunk(hunkOf(row.at)!.at);
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
        {editable(row) && side === "new" ? (
          <button type="button" className="pd-hb ic1 pd-pen" tabIndex={cursor === row.at ? 0 : -1} aria-label="Edit this line" title="Edit this line" onClick={() => edit(row)}>
            <SquarePen size={12} aria-hidden />
          </button>
        ) : null}
      </div>
    );
  };

  /** The line Edit a line takes: the one the keyboard is on or the pointer last touched in this hunk, else its first added line. */
  const editTarget = (hunk: Hunk) => {
    const inHunk = (at: number | null) => (at === null ? undefined : hunk.rows.find((row) => row.at === at && editable(row)));
    return inHunk(focused ? cursor : null) ?? inHunk(lastRow.current) ?? hunk.rows.find((row) => row.line.kind === "add" && editable(row));
  };
  const hunkHeader = (row: Row) => {
    const hunk = hunks.find((entry) => entry.at === row.at);
    const active = overHunk === row.at || (focused && cursor !== null && hunkOf(cursor)?.at === row.at);
    const offered = actions?.revertible === true && hunk !== undefined && canRevert(hunk);
    const copy = () => void navigator.clipboard?.writeText(hunkPatch(hunk!)).then(() => pushToast("Hunk copied"));
    return (
      <div key={row.at} className={`hh2 pd-hh${active ? " on" : ""}`} data-row={row.at}>
        <span>{hunkLabel(row.line.text)}</span>
        {offered ? (
          <span className="pd-ha">
            {narrow ? (
              <MenuButton
                menuLabel="Hunk actions"
                align="end"
                width={190}
                items={[
                  { label: "Revert hunk", icon: <Undo2 size={14} aria-hidden />, onSelect: () => setConfirm(hunk) },
                  ...(editTarget(hunk) ? [{ label: "Edit a line…", icon: <SquarePen size={14} aria-hidden />, onSelect: () => edit(editTarget(hunk)!) }] : []),
                  { label: "Copy hunk", icon: <Copy size={14} aria-hidden />, onSelect: copy },
                ]}
              >
                {(props) => (
                  <IconButton label="Hunk actions" {...props}>
                    <MoreHorizontal size={14} aria-hidden />
                  </IconButton>
                )}
              </MenuButton>
            ) : (
              <>
                <button type="button" className="pd-hb" onClick={() => setConfirm(hunk)}>
                  <Undo2 size={12} aria-hidden />
                  Revert hunk
                </button>
                {editTarget(hunk) ? (
                  <button type="button" className="pd-hb ic1" aria-label="Edit a line" title="Edit a line" onClick={() => edit(editTarget(hunk)!)}>
                    <SquarePen size={12} aria-hidden />
                  </button>
                ) : null}
                <button type="button" className="pd-hb ic1" aria-label="Copy hunk" title="Copy hunk" onClick={copy}>
                  <Copy size={12} aria-hidden />
                </button>
              </>
            )}
          </span>
        ) : null}
      </div>
    );
  };

  const unifiedCell = (row: Row) => {
    const kind = row.line.kind;
    if (kind === "hunk") return hunkHeader(row);
    if (kind === "meta") return <div key={row.at} className="hh2" data-row={row.at}>{row.line.text}</div>;
    // A removed line numbers on the old side; the rest on the new side, where a comment can be made.
    return cell(row, kind === "del" ? "old" : "new", false);
  };

  const after = (row: Row | null) => {
    if (!row || row.newNo === null) return null;
    const under = comments.filter((chip) => (chip.endLine ?? chip.line) === row.newNo);
    const form = pick !== null && commenting && row.at === high;
    const bar = pick !== null && !commenting && row.at === high;
    const editBar = editing?.at === row.at ? row : null;
    if (under.length === 0 && !form && !bar && !editBar) return null;
    return (
      <>
        {editBar ? (
          <div className="pd-ebar">
            Esc cancels · ⌘↵ saves to the file
            <span className="end">
              <Button small tone="quiet" onClick={() => setEditing(null)}>Cancel</Button>
              <Button small tone="primary" disabled={saving} onClick={() => void save(editBar)}>Save line</Button>
            </span>
          </div>
        ) : null}
        {bar ? (
          range ? (
            <div className="pd-sbar" role="toolbar" aria-label="Selection">
              <button type="button" onClick={() => setCommenting(true)}><MessageSquare size={13} aria-hidden />Comment</button>
              <button type="button" className="on" onClick={send}><ArrowUp size={13} aria-hidden />Send selection to engine</button>
              <button type="button" aria-label="Copy" title="Copy" onClick={copySelection}><Copy size={13} aria-hidden /></button>
            </div>
          ) : (
            <p role="alert" className="t3 pd-sbar-note">Select at most 4,000 characters of the diff for a comment.</p>
          )
        ) : null}
        {bar && error ? <p role="alert" className="t3 pd-sbar-note">{error}</p> : null}
        {under.map((chip) => (
          <div key={chip.id} className="cmt2" role="note" aria-label={`Review comment ${chipLabel(chip)}`}>
            <span>
              <b>You</b> · {chip.comment} <span className="t3">{sent.includes(chip) ? "Sent to the agent" : "Sent with your next message"}</span>
            </span>
            {sent.includes(chip) ? null : (
              <div className="row">
                <Button small tone="quiet" onClick={() => useComposerChips.getState().remove(threadId, chip.id)}>
                  Remove comment
                </Button>
              </div>
            )}
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
              <span className="t3 grow">Sent with your next message</span>
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
        onPointerLeave={() => setOverHunk(null)}
        aria-describedby={hintId}
        onFocus={() => setFocused(true)}
        onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false); }}
      >
        {body}
      </div>
      {confirm && actions ? (
        <ConfirmDialog
          title="Revert this hunk?"
          confirmLabel="Revert hunk"
          tone="danger"
          cancelFocus
          onClose={() => setConfirm(null)}
          onConfirm={() => actions.revert(confirm)}
        >
          <p>
            {changedLines(confirm)} of {path} {changedLines(confirm).startsWith("Lines") ? "go" : "goes"} back to the last commit. You can undo it for 10 seconds afterwards.
          </p>
        </ConfirmDialog>
      ) : null}
      <span id={hintId} className="sr-only">Arrow keys move between lines, C comments on the line, Shift and an arrow extend the range.</span>
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
