import { Search } from "lucide-react";
import { useRef, type KeyboardEvent } from "react";
import type { FileDiff } from "../../../api/types";
import { ChangeLetter, MiddleName } from "../../workspace";
import { changeOf, isBinaryPatch, renameOf } from "./diffModel";

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
export function ChangedFiles({ files, selected, filter, onFilter, onPick }: {
  files: readonly FileDiff[];
  selected: string | null;
  filter: string;
  onFilter(value: string): void;
  onPick(path: string): void;
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
            title={fileName(file)}
            onClick={() => onPick(file.path)}
          >
            <ChangeLetter change={changeOf(file)} />
            {file.status === "renamed" ? <span className="nm">{fileName(file)}</span> : <MiddleName text={file.path} middle />}
            <span className="num">{fileNumbers(file)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
