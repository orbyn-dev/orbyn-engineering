import { useState, type KeyboardEvent } from "react";
import { rowWhen, searchHistory, type HistoryEntry } from "./history";

export type HistoryScope = "shell" | "project";

/**
 * The history search: a list that narrows as the person types, with the matching part bold. The scope chip says
 * whose history it is, and widens from this shell to this project's terminals. The row text is masked; the real
 * command is what a pick returns.
 */
export function HistoryPop({ shell, project, scope: first, onPick, onClose }: { shell: readonly HistoryEntry[]; project: readonly HistoryEntry[]; scope: HistoryScope; onPick(command: string): void; onClose(): void }) {
  const [scope, setScope] = useState(first);
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const entries = scope === "shell" ? shell : project;
  const found = searchHistory(entries, query);
  const row = found[at];

  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.ctrlKey && event.key.toLowerCase() === "r") {
      event.preventDefault();
      setScope(event.shiftKey ? "project" : "shell");
      setAt(0);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (found.length > 0) setAt((at + (event.key === "ArrowDown" ? 1 : found.length - 1)) % found.length);
    } else if ((event.key === "Enter" || event.key === "Tab") && row) {
      event.preventDefault();
      onPick(row.entry.command);
    }
  };

  return (
    <div className="term-input-pop" role="dialog" aria-label="Search history">
      <div className="term-input-scope">
        <button type="button" className="text-action" aria-label={scope === "shell" ? "This shell, widen to this project" : "This project, narrow to this shell"} onClick={() => { setScope(scope === "shell" ? "project" : "shell"); setAt(0); }}>
          {scope === "shell" ? "This shell" : "This project"}
        </button>
        <input autoFocus aria-label="Search history" value={query} onChange={(event) => { setQuery(event.target.value); setAt(0); }} onKeyDown={onKey} />
      </div>
      {found.length === 0 ? <p className="term-input-none">{entries.length === 0 ? (scope === "shell" ? "Nothing run from this editor yet." : "Nothing run from this project's editors yet.") : "No match."}</p> : null}
      <ul role="listbox" aria-label="History">
        {found.map((hit, index) => (
          <li key={hit.entry.command} role="option" aria-selected={index === at}>
            <code>
              {hit.match ? (
                <>
                  {hit.shown.slice(0, hit.match[0])}
                  <b>{hit.shown.slice(hit.match[0], hit.match[1])}</b>
                  {hit.shown.slice(hit.match[1])}
                </>
              ) : (
                hit.shown
              )}
            </code>
            <span className="term-input-when">{rowWhen(hit.entry)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
