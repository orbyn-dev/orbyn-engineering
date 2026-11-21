import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Thread } from "../../api/types";
import { ApiFailure } from "../../api/client";
import { useWaited } from "../../lib/motion";
import { navigate } from "../../product/state/route";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { issuesApi, useTrackers } from "./api";
import { TRACKER_NAMES, type IssueRef } from "./contract";
import { IssueGlyph } from "./IssueGlyph";
import { useThreadIssues } from "./links";
import { isTracker, notFoundWords, reasonOf, refKey, stateWord, trackerName } from "./model";
import "./issues.css";

const SEARCH_MS = 150;

/**
 * Link an issue to this thread, from the Details card. A search over the issues of every connected tracker, merged (a number, words
 * or a pasted issue link), the latest issues at first. An issue this thread already has is not offered. Choosing a row writes nothing;
 * Link is the one write, and a second press while it is in flight is not a second write.
 */
export function LinkIssueDialog({ thread, onClose }: { thread: Thread; onClose(): void }) {
  const { rows, settled } = useTrackers();
  const { issues: linked, reload } = useThreadIssues(thread.id);
  const [value, setValue] = useState("");
  const [found, setFound] = useState<IssueRef[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [chosen, setChosen] = useState<IssueRef | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const latest = useRef(0);
  const list = useRef<HTMLUListElement>(null);
  const connected = rows?.filter((row) => row.connected) ?? [];
  // Nothing is searched until the trackers are known; a service that reports none has nothing to search either.
  const noTracker = settled && connected.length === 0;

  // Search as the person types, after a short pause, and ignore an answer that a newer search has passed.
  useEffect(() => {
    if (!settled || noTracker) return;
    const id = ++latest.current;
    setFound(null);
    setFailed(false);
    const timer = setTimeout(() => {
      issuesApi().find(value.trim()).then(
        (issues) => { if (id === latest.current) setFound(issues); },
        () => { if (id === latest.current) setFailed(true); },
      );
    }, value.trim() === "" ? 0 : SEARCH_MS);
    return () => clearTimeout(timer);
  }, [value, attempt, noTracker, settled]);
  const waited = useWaited(found === null && !failed);
  const have = new Set(linked.map(refKey));
  const options = (found ?? []).filter((issue) => !have.has(refKey(issue)));

  const link = async () => {
    if (!chosen || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await issuesApi().link(thread.id, chosen);
      await reload();
      onClose();
    } catch (failure) {
      setError(failure instanceof ApiFailure && failure.status === 404 ? notFoundWords(chosen.display_key, chosen) : `Couldn’t link it. Nothing changed. ${reasonOf(failure)}`);
      setBusy(false);
    } finally {
      pending.current = false;
    }
  };
  const move = (event: KeyboardEvent<HTMLElement>, step: number) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>("[role=option]") ?? [])];
    const at = items.indexOf(event.currentTarget);
    event.preventDefault();
    items[Math.max(0, Math.min(items.length - 1, at + step))]?.focus();
  };

  if (noTracker) {
    return (
      <Dialog title="Link an issue" onClose={onClose} className="iss-dialog"
        footer={<><Button onClick={onClose}>Cancel</Button><Button tone="primary" data-dialog-focus onClick={() => { onClose(); navigate({ name: "settings", section: "source-control" }); }}>Open Settings</Button></>}>
        <div className="iss-dlg-empty">
          <h3>Connect a tracker</h3>
          <p>Orbyn needs one tracker to find issues. GitHub uses your gh sign-in; Azure DevOps, Forgejo, GitLab and Linear use an API token.</p>
        </div>
      </Dialog>
    );
  }
  const names = connected.flatMap((row) => (isTracker(row.tracker) ? [TRACKER_NAMES[row.tracker]] : []));
  const said = names.length === 0 ? "Your trackers" : names.length === 1 ? names[0]! : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
  return (
    <Dialog title="Link an issue" busy={busy} onClose={onClose} className="iss-dialog"
      footer={<><Button disabled={busy} onClick={onClose}>Cancel</Button><Button tone="primary" busy={busy} disabled={!chosen} onClick={() => void link()}>Link</Button></>}>
      <input
        className="iss-dlg-search"
        type="search"
        role="combobox"
        aria-label="Search issues"
        aria-expanded={options.length > 0}
        aria-controls="iss-dlg-options"
        aria-autocomplete="list"
        placeholder="Issue number, words or a link"
        data-dialog-focus
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(event) => { setValue(event.target.value); setChosen(null); setError(null); }}
        onKeyDown={(event) => { if (event.key === "ArrowDown") { event.preventDefault(); list.current?.querySelector<HTMLElement>("[role=option]")?.focus(); } }}
      />
      {failed ? (
        <div className="iss-dlg-empty" role="alert">
          <h3>Can’t read your trackers</h3>
          <p>{said} did not answer. Nothing is changed. Retry asks again.</p>
          <Button small onClick={() => setAttempt(attempt + 1)}>Retry</Button>
        </div>
      ) : found === null ? (
        <div className="iss-dlg-list" aria-busy>{waited ? [72, 58, 66].map((width) => <span key={width} className="iss-bar" style={{ width: `${width}%`, height: 14 }} aria-hidden />) : null}</div>
      ) : options.length === 0 ? (
        <p className="iss-quiet iss-dlg-none">{value.trim() === "" ? "No open issue to link." : "No issue matches."}</p>
      ) : (
        <ul id="iss-dlg-options" ref={list} role="listbox" aria-label="Matching issues" className="iss-dlg-list">
          {options.map((option) => {
            const on = chosen !== null && refKey(chosen) === refKey(option);
            return (
              <li
                key={refKey(option)}
                role="option"
                aria-selected={on}
                tabIndex={0}
                className="iss-dlg-row"
                onClick={() => { setChosen(option); setError(null); }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") { event.preventDefault(); setChosen(option); setError(null); }
                  else if (event.key === "ArrowDown") move(event, 1);
                  else if (event.key === "ArrowUp") move(event, -1);
                }}
              >
                <span className="iss-dlg-line"><IssueGlyph state={option.state} /><span className="iss-key">{option.display_key}</span><b title={option.title}>{option.title}</b></span>
                <span className="iss-quiet iss-dlg-sub">{trackerName(option.tracker)} · {option.scope}{option.state.category === "closed" ? ` · ${stateWord(option.state)}` : ""}</span>
              </li>
            );
          })}
        </ul>
      )}
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </Dialog>
  );
}
