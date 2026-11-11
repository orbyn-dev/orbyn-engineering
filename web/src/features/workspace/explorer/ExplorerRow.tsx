import { ChevronDown, ChevronRight, CircleDashed, File, Folder, GitBranch, MoreHorizontal } from "lucide-react";
import { memo, type CSSProperties, type KeyboardEvent, type MouseEvent } from "react";
import { compactAge } from "../../../lib/format";
import { useWaited } from "../../../lib/motion";
import { attentionRank } from "../sidebar/groups";
import { CHANGE, MAX_DEPTH, crumb, type Row } from "./model";
import { FILE_MENTION_TYPE } from "../../../product/composer/skillMention";
import { Button } from "../../../ui/controls";

/** A name that gives way in the middle, so the ending that tells two files apart stays in view. */
function Name({ text, middle }: { text: string; middle: boolean }) {
  if (!middle || text.length <= 10) return <span className="nm">{text}</span>;
  return (
    <span className="nm nm-mid">
      <span className="nm-head">{text.slice(0, -7)}</span>
      <span className="nm-tail">{text.slice(-7)}</span>
    </span>
  );
}

function ThreadStatus({ thread }: { thread: NonNullable<Row["thread"]> }) {
  if (attentionRank(thread) === 0) return <span className="n"><span className="status-dot tone-warn" role="img" aria-label="Needs you" /></span>;
  if (thread.active_run) return <span className="n"><CircleDashed size={12} className="spin" role="img" aria-label="Working" /></span>;
  if (thread.last_run_state === "failed") return <span className="n x2failed">failed</span>;
  return <span className="n">{compactAge(thread.updated_at)}</span>;
}

/** Quiet placeholder rows, drawn only if the read outlasts 400 ms; a quicker one shows nothing. */
function LoadingRow({ label, common }: { label: string; common: object }) {
  const slow = useWaited(true);
  return (
    <div {...common} role="treeitem" aria-disabled="true" aria-label={label} tabIndex={-1}>
      {slow ? (
        <>
          <div className="x2sk" />
          <div className="x2sk" />
        </>
      ) : null}
    </div>
  );
}

export interface RowHandlers {
  onActivate(row: Row, event: MouseEvent | KeyboardEvent): void;
  onMenu(row: Row, x: number, y: number): void;
  onFocus(row: Row): void;
}

/** One line of the tree. Click opens or folds; a file opens as a tab; the row is one tab stop, moved by the arrows. */
export const ExplorerRow = memo(function ExplorerRow({
  row,
  selected,
  focusable,
  menuOpen,
  dragPath,
  place,
  handlers,
}: {
  row: Row;
  selected: boolean;
  focusable: boolean;
  menuOpen: boolean;
  /** A file of the open thread's own folder can be dragged into its message, by this path. */
  dragPath: string | null;
  /** Its place among its siblings, for a list that draws only some of its rows. */
  place?: { pos: number; size: number };
  handlers: RowHandlers;
}) {
  const style = { "--d": Math.min(row.depth, MAX_DEPTH) } as CSSProperties;
  const common = { "data-key": row.key, style, tabIndex: focusable ? 0 : -1, onFocus: () => handlers.onFocus(row) } as const;

  if (row.kind === "loading") return <LoadingRow label={row.label} common={common} />;
  if (row.kind === "empty") {
    return (
      <div {...common} className="x2r x2note" role="treeitem" aria-disabled="true" aria-level={row.depth + 1} tabIndex={-1}>
        <span className="nm">{row.label}</span>
      </div>
    );
  }
  if (row.kind === "error") {
    return (
      <div {...common} className="x2banner err" role="treeitem" aria-disabled="true" aria-level={row.depth + 1} aria-label={row.label} tabIndex={-1}>
        <span className="berr">{row.label}</span>
        <Button small tone="quiet" title={row.title} onClick={(event) => handlers.onActivate(row, event)}>
          Try again
        </Button>
      </div>
    );
  }
  if (row.kind === "more") {
    return (
      <div {...common} className="x2r x2more" role="treeitem" aria-level={row.depth + 1} onClick={(event) => handlers.onActivate(row, event)}>
        <span className="nm">{row.label}</span>
      </div>
    );
  }

  const expandable = row.kind === "project" || row.kind === "group" || row.kind === "dir" || (row.kind === "worktree" && !row.missing);
  const cls = ["x2r", row.kind === "thread" ? "x2t" : "", row.kind === "group" ? "x2g" : "", selected ? "on" : "", row.current ? "cur" : "", menuOpen ? "mopen" : "", row.missing ? "xg-miss" : "", row.change === "conflicted" ? "xg-c" : "", row.change === "deleted" ? "xg-d" : ""]
    .filter(Boolean)
    .join(" ");
  const deep = row.depth > MAX_DEPTH;
  const title = deep && row.path ? `${row.label} · ${crumb(row.path)}` : row.title ?? row.label;
  return (
    <div
      {...common}
      className={cls}
      role="treeitem"
      aria-level={row.depth + 1}
      aria-expanded={expandable ? row.open : undefined}
      aria-selected={selected}
      // A name split for the narrow bar is read whole, with its change.
      aria-label={row.kind === "file" ? `${row.label}${row.change && CHANGE[row.change] ? `, ${CHANGE[row.change]!.label}` : ""}` : row.missing ? `${row.label}, missing from disk` : undefined}
      title={title}
      aria-posinset={place?.pos}
      aria-setsize={place?.size}
      draggable={dragPath !== null || undefined}
      onDragStart={dragPath !== null ? (event) => event.dataTransfer.setData(FILE_MENTION_TYPE, dragPath) : undefined}
      onClick={(event) => handlers.onActivate(row, event)}
      onContextMenu={(event) => {
        if (row.kind !== "file" && row.kind !== "worktree") return;
        event.preventDefault();
        handlers.onMenu(row, event.clientX, event.clientY);
      }}
    >
      {expandable ? (
        row.open ? <ChevronDown size={11} className="xs" aria-hidden /> : <ChevronRight size={11} className="xs" aria-hidden />
      ) : null}
      {row.kind === "worktree" ? <GitBranch size={13} className="ic" aria-hidden /> : null}
      {row.kind === "dir" ? <Folder size={13} className="ic" aria-hidden /> : null}
      {row.kind === "file" ? <File size={13} className="ic" aria-hidden /> : null}
      <Name text={row.label} middle={row.kind === "file"} />
      {row.kind === "project" && (row.needs ?? 0) > 0 ? <span className="n amber">{row.needs}</span> : null}
      {row.kind === "worktree" && row.current ? <span className="xdot" role="img" aria-label="Current worktree" title="Current worktree" /> : null}
      {row.badge ? <span className="n x2ro">{row.badge}</span> : null}
      {row.kind === "thread" && row.thread ? <ThreadStatus thread={row.thread} /> : null}
      {row.kind === "worktree" && !row.missing ? (
        <button
          type="button"
          className="x2more-btn"
          aria-label={`Actions for ${row.label}`}
          aria-haspopup="menu"
          tabIndex={-1}
          onClick={(event) => {
            event.stopPropagation();
            const rect = event.currentTarget.getBoundingClientRect();
            handlers.onMenu(row, rect.left, rect.bottom + 4);
          }}
        >
          <MoreHorizontal size={14} aria-hidden />
        </button>
      ) : null}
      {row.kind === "file" && row.change && CHANGE[row.change] ? (
        <span className={`xm ${row.change === "modified" ? "m" : row.change === "added" ? "a" : row.change === "renamed" ? "r" : row.change === "untracked" ? "u" : row.change === "deleted" ? "d" : row.change === "conflicted" ? "xg-bang" : ""}`} title={CHANGE[row.change]!.label} aria-label={CHANGE[row.change]!.label}>
          {CHANGE[row.change]!.letter}
        </span>
      ) : null}
    </div>
  );
});
