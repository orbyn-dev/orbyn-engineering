import { useRef, useState, type KeyboardEvent } from "react";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import type { IssueDetail } from "./contract";
import { reasonOf } from "./model";
import "./issues.css";

/**
 * Links an existing thread to this issue, from the Linked threads card: one write, `POST /api/threads/{id}/issue-links`, through
 * the page's own write guard, then the links are read again from a read that starts after it. The threads are the app's own list,
 * so nothing is read here; a thread this issue already has is not offered. Choosing a row writes nothing; Link is the one write,
 * and a second press while it is in flight is not a second. The search declares a combobox and the rows a listbox, so the arrow
 * keys walk the rows and Enter or Space chooses one, as Link an issue does. No frame draws this picker: it follows that dialog's
 * form.
 */
export function LinkThreadDialog({ issue, link, onClose, onLinked }: { issue: IssueDetail; link(threadId: string): Promise<boolean>; onClose(): void; onLinked(): void }) {
  const threads = useWorkspace((state) => state.threads);
  const projects = useWorkspace((state) => state.projects.data);
  const [filter, setFilter] = useState("");
  const [chosen, setChosen] = useState<Thread | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const list = useRef<HTMLUListElement>(null);
  const linked = new Set(issue.linked_threads);
  const wanted = filter.trim().toLowerCase();
  const candidates = Object.values(threads)
    .filter((thread) => !linked.has(thread.id))
    .sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at));
  const options = wanted === "" ? candidates : candidates.filter((thread) => thread.title.toLowerCase().includes(wanted));
  const linkIt = async () => {
    if (!chosen || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      const done = await link(chosen.id);
      if (!done) {
        // The page's one write at a time: another control's write is still in flight, so nothing was sent.
        setError("Couldn’t link it. Nothing changed. The page is still saving another change.");
        setBusy(false);
        pending.current = false;
        return;
      }
      onLinked();
      onClose();
    } catch (failure) {
      setError(`Couldn’t link it. Nothing changed. ${reasonOf(failure)}`);
      setBusy(false);
      pending.current = false;
    }
  };
  const move = (event: KeyboardEvent<HTMLElement>, step: number) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>("[role=option]") ?? [])];
    const at = items.indexOf(event.currentTarget);
    event.preventDefault();
    items[Math.max(0, Math.min(items.length - 1, at + step))]?.focus();
  };
  return (
    <Dialog title="Link a thread" busy={busy} onClose={onClose} className="iss-dialog"
      footer={<><Button disabled={busy} onClick={onClose}>Cancel</Button><Button tone="primary" busy={busy} disabled={!chosen} onClick={() => void linkIt()}>Link</Button></>}>
      <input
        className="iss-dlg-search"
        type="search"
        role="combobox"
        aria-label="Search threads"
        aria-expanded={options.length > 0}
        aria-controls="iss-thread-options"
        aria-autocomplete="list"
        placeholder="Thread title"
        data-dialog-focus
        autoComplete="off"
        spellCheck={false}
        value={filter}
        onChange={(event) => { setFilter(event.target.value); setChosen(null); setError(null); }}
        onKeyDown={(event) => { if (event.key === "ArrowDown") { event.preventDefault(); list.current?.querySelector<HTMLElement>("[role=option]")?.focus(); } }}
      />
      {options.length === 0 ? (
        <p className="iss-quiet iss-dlg-none">{candidates.length === 0 ? "No thread to link." : "No thread matches."}</p>
      ) : (
        <ul id="iss-thread-options" ref={list} role="listbox" aria-label="Threads" className="iss-dlg-list">
          {options.map((thread) => {
            const project = projects?.find((one) => one.id === thread.project_id)?.name ?? "";
            return (
              <li
                key={thread.id}
                role="option"
                aria-selected={chosen?.id === thread.id}
                tabIndex={0}
                className="iss-dlg-row"
                onClick={() => { setChosen(thread); setError(null); }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") { event.preventDefault(); setChosen(thread); setError(null); }
                  else if (event.key === "ArrowDown") move(event, 1);
                  else if (event.key === "ArrowUp") move(event, -1);
                }}
              >
                <span className="iss-dlg-line"><b title={thread.title}>{thread.title}</b></span>
                <span className="iss-quiet iss-dlg-sub">{[project, thread.engine, thread.active_run ? "Running" : ""].filter(Boolean).join(" · ")}</span>
              </li>
            );
          })}
        </ul>
      )}
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </Dialog>
  );
}
