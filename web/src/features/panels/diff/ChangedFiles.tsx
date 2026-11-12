import { Search } from "lucide-react";
import { useRef, type KeyboardEvent } from "react";
import type { FileDiff } from "../../../api/types";
import { ChangeLetter, MiddleName } from "../../workspace";
import { Button } from "../../../ui/controls";
import { changeOf, isBinaryPatch, renameOf } from "./diffModel";

/** What a row says before its name, so the name reads as one phrase. */
const KIND = { modified: "Modified", added: "Added", deleted: "Deleted", renamed: "Renamed", untracked: "Untracked", conflicted: "Conflicted" } as const;

const count = (value: number) => value.toLocaleString("en-US");

/** What a file's row says at its end: its counts, the similarity of a rename, or that it is in conflict. */
function fileNumbers(file: FileDiff): string {
  if (file.status === "conflicted") return "conflict";
  if (file.status === "renamed") {
    const similarity = renameOf(file.patch)?.similarity;
    if (similarity != null) return `${similarity}%`;
  }
  // A binary file has no lines to count and its sizes are not reported, so its row ends at its name.
  if (isBinaryPatch(file.patch)) return "";
  return `+${count(file.additions)} −${count(file.deletions)}`;
}

/** A renamed file's row names the new path and the old one it came from. */
export function fileName(file: FileDiff): string {
  const from = file.status === "renamed" ? renameOf(file.patch)?.from : null;
  return from ? `${file.path} ← ${from}` : file.path;
}

/** The filter and the changed files, one row each. The row of the file on show is marked, and the arrow keys move between rows. */
export function ChangedFiles({ files, more, selected, filter, onFilter, onPick, onMore }: {
  files: readonly FileDiff[];
  /** How many files the list has not drawn yet; Show adds all of them. */
  more: number;
  selected: string | null;
  filter: string;
  onFilter(value: string): void;
  onPick(path: string): void;
  onMore(): void;
}) {
  const list = useRef<HTMLDivElement>(null);
  const onKeyDown = (event: KeyboardEvent) => {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    const rows = [...(list.current?.querySelectorAll<HTMLElement>("button.fli") ?? [])];
    const at = rows.indexOf(document.activeElement as HTMLElement);
    const next = rows[at < 0 ? (step > 0 ? 0 : rows.length - 1) : Math.min(Math.max(at + step, 0), rows.length - 1)];
    if (!next) return;
    event.preventDefault();
    next.focus();
  };
  return (
    <div className="fl2">
      <label className="flf">
        <Search size={14} aria-hidden />
        <input type="search" aria-label="Filter files" placeholder="Filter files" value={filter} onChange={(event) => onFilter(event.target.value)} />
      </label>
      <div ref={list} role="group" aria-label="Changed files" onKeyDown={onKeyDown}>
        {files.map((file) => (
          <button
            key={file.path}
            type="button"
            className={`fli ${file.path === selected ? "on" : ""}`}
            data-diff-row={file.path}
            aria-current={file.path === selected || undefined}
            tabIndex={file.path === (selected ?? files[0]?.path) ? 0 : -1}
            title={fileName(file)}
            aria-label={`${KIND[changeOf(file)]} ${fileName(file)}${fileNumbers(file) ? `, ${fileNumbers(file)}` : ""}`}
            onClick={() => onPick(file.path)}
          >
            <ChangeLetter change={changeOf(file)} />
            {file.status === "renamed" ? <span className="nm">{fileName(file)}</span> : <MiddleName text={file.path} middle />}
            <span className="num">{fileNumbers(file)}</span>
          </button>
        ))}
        {more > 0 ? (
          <div className="x2banner">
            <span>{more === 1 ? "1 more file not shown." : `${more} more files not shown.`}</span>
            <span className="sp" />
            <Button small tone="quiet" onClick={onMore}>{more === 1 ? "Show 1 more file" : `Show ${more} more files`}</Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
