import { Plus, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type { Thread } from "../../api/types";
import { ApiFailure } from "../../api/client";
import { toFailure } from "../../lib/useAsync";
import { useLayoutWidth } from "../../lib/useLayout";
import { navigate } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { Button, IconButton } from "../../ui/controls";
import { ToolbarPopover } from "../pulls";
import { issuesApi, useTrackers } from "./api";
import type { IssueRef } from "./contract";
import { IssueGlyph } from "./IssueGlyph";
import { useThreadIssues } from "./links";
import { notFoundWords, numberOf, refKey, trackerName } from "./model";

const LONG_PRESS_MS = 500;

function Row({ thread, issue, phone, onOpen, onUnlinked }: { thread: Thread; issue: IssueRef; phone: boolean; onOpen(): void; onUnlinked(): Promise<void> }) {
  const pushToast = useUi((state) => state.pushToast);
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const unlink = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await issuesApi().unlink(thread.id, issue);
      await onUnlinked();
    } catch (failure) {
      setBusy(false);
      pushToast(`Couldn't do that. ${toFailure(failure).message}`);
    }
  };
  return (
    <div className="iss-prow" data-armed={armed || undefined}>
      <button
        type="button"
        className="iss-prow-open"
        aria-label={`${issue.display_key} ${issue.title}`}
        // On a phone, press and hold a row to reach Unlink.
        onPointerDown={phone ? () => { timer.current = setTimeout(() => setArmed(true), LONG_PRESS_MS); } : undefined}
        onPointerUp={phone ? () => clearTimeout(timer.current) : undefined}
        onPointerLeave={phone ? () => clearTimeout(timer.current) : undefined}
        onContextMenu={phone ? (event) => { event.preventDefault(); setArmed(true); } : undefined}
        onClick={() => { onOpen(); navigate({ name: "issues", tracker: issue.tracker, scope: issue.scope, key: issue.key }); }}
      >
        <span className="iss-prow-title"><span className="iss-key">{issue.display_key}</span><b>{issue.title}</b></span>
        <span className="iss-prow-meta">
          <span className="iss-state"><IssueGlyph state={issue.state} size={12} />{issue.state.category === "open" ? "Open" : issue.state.close_reason === "not_planned" ? "Closed as not planned" : "Closed"}</span>
          <span>{trackerName(issue.tracker)}</span>
          {issue.labels.slice(0, 3).map((label, at) => <span key={`${at}:${label.name}`} className="iss-label">{label.name}</span>)}
        </span>
      </button>
      <button type="button" className="iss-unlink" aria-label={`Unlink ${issue.display_key}`} disabled={busy} onClick={() => void unlink()}>Unlink</button>
    </div>
  );
}

/**
 * Link an issue: a number, words or a pasted issue link. Matches are searched in the connected trackers and chosen from the list;
 * nothing is linked until Link is pressed, once. A number that is no issue says so under the field and links nothing.
 */
export function LinkForm({ thread, onDone, onCancel }: { thread: Thread; onDone(): void; onCancel(): void }) {
  const { reload } = useThreadIssues(thread.id);
  const { rows } = useTrackers();
  const [value, setValue] = useState("");
  const [options, setOptions] = useState<IssueRef[]>([]);
  const [chosen, setChosen] = useState<IssueRef | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Search as the person types, after a short pause, and ignore an answer that a newer search has passed.
  const latest = useRef(0);
  useEffect(() => {
    const text = value.trim();
    const id = ++latest.current;
    if (!text) { setOptions([]); return; }
    const timer = setTimeout(() => {
      issuesApi().find(text).then((found) => { if (id === latest.current) setOptions(found); }, () => { if (id === latest.current) setOptions([]); });
    }, 150);
    return () => clearTimeout(timer);
  }, [value]);
  const first = rows?.find((row) => row.connected) ?? null;
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setError(null);
    let target = chosen;
    if (!target) {
      const text = value.trim();
      const exact = options.filter((option) => numberOf(text) !== null ? option.key === numberOf(text) : option.display_key.toLowerCase() === text.toLowerCase());
      if (exact.length === 1) target = exact[0]!;
      else if (options.length > 1 || exact.length > 1) { setError("Choose one of the issues listed."); return; }
      else {
        const shown = numberOf(text) !== null ? `#${numberOf(text)}` : text;
        setError(first ? notFoundWords(shown, { scope: first.scope ?? "", tracker: first.tracker }) : "No tracker is connected.");
        return;
      }
    }
    setBusy(true);
    try {
      await issuesApi().link(thread.id, target);
      await reload();
      onDone();
    } catch (failure) {
      setBusy(false);
      setError(failure instanceof ApiFailure && failure.status === 404 ? notFoundWords(target.display_key, target) : toFailure(failure).message);
    }
  };
  return (
    <form className="iss-link" onSubmit={(event) => void submit(event)}>
      <div className="iss-pop-head">Link an issue</div>
      <label className="iss-link-label">
        <span>Issue number or link</span>
        <input
          role="combobox"
          aria-label="Issue number or link"
          aria-expanded={options.length > 0}
          aria-controls="iss-link-options"
          aria-autocomplete="list"
          autoFocus
          value={value}
          data-invalid={error ? "" : undefined}
          onChange={(event) => { setValue(event.target.value); setChosen(null); setError(null); }}
        />
      </label>
      {options.length > 0 ? (
        <ul id="iss-link-options" role="listbox" aria-label="Matching issues" className="iss-options">
          {options.map((option) => (
            <li key={refKey(option)} role="option" aria-selected={chosen !== null && refKey(chosen) === refKey(option)} tabIndex={-1} onClick={() => { setChosen(option); setValue(`${option.display_key} ${option.title}`); setOptions([option]); }}>
              <span className="iss-key">{option.display_key}</span><b>{option.title}</b>
              <span className="iss-state"><IssueGlyph state={option.state} size={12} />{option.state.category === "open" ? "Open" : "Closed"}</span><span className="iss-quiet">{trackerName(option.tracker)}</span>
            </li>
          ))}
        </ul>
      ) : <p className="iss-quiet">Search by number or words. Or paste an issue link.</p>}
      {error ? <p className="iss-link-error" role="alert">{error}</p> : null}
      <div className="iss-link-acts">
        <Button small onClick={onCancel}>Cancel</Button>
        <Button small tone="primary" type="submit" busy={busy}>Link</Button>
      </div>
    </form>
  );
}

function IssueList({ thread, onOpen, sheet = false }: { thread: Thread; onOpen(): void; sheet?: boolean }) {
  const { issues, reload } = useThreadIssues(thread.id);
  const [linking, setLinking] = useState(false);
  const phone = useLayoutWidth() === "phone";
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => { root.current?.querySelector<HTMLElement>(".iss-prow-open")?.focus(); }, []);
  if (linking) return <div className="iss-pop" data-sheet={sheet || undefined}><LinkForm thread={thread} onDone={() => setLinking(false)} onCancel={() => setLinking(false)} /></div>;
  return (
    <div className="iss-pop" ref={root} data-sheet={sheet || undefined}>
      <div className="iss-pop-head">{issues.length === 1 ? "1 linked issue" : `${issues.length} linked issues`}</div>
      {issues.map((issue) => <Row key={refKey(issue)} thread={thread} issue={issue} phone={phone} onOpen={onOpen} onUnlinked={reload} />)}
      <button type="button" className="iss-link-open" aria-expanded={linking} onClick={() => setLinking(true)}><Plus size={14} aria-hidden />Link an issue…</button>
    </div>
  );
}

/**
 * The header's issue chip, left of the pull request chip: the key and the state's shape for one issue, "Issues 3" for
 * several. A thread with none linked has no chip. It opens the list as a popover, or a bottom sheet on a phone.
 */
export function IssueChip({ thread }: { thread: Thread }) {
  const { issues } = useThreadIssues(thread.id);
  const [open, setOpen] = useState<{ anchor: HTMLElement; sheet: boolean } | null>(null);
  const sheet = useLayoutWidth() === "phone";
  if (issues.length === 0) return null;
  const one = issues.length === 1 ? issues[0]! : null;
  const everyClosed = issues.every((issue) => issue.state.category === "closed");
  return (
    <>
      <button
        type="button"
        className="iss-chip-btn"
        aria-label="Issues"
        aria-haspopup="dialog"
        aria-expanded={open !== null}
        data-pressed={open !== null || undefined}
        onClick={(event) => setOpen(open ? null : { anchor: event.currentTarget, sheet })}
      >
        <IssueGlyph state={one?.state ?? { category: everyClosed ? "closed" : "open", name: "" }} />
        {one ? one.display_key : `Issues ${issues.length}`}
      </button>
      {open ? (
        <ToolbarPopover anchor={open.anchor} label="Issues" layout={open.sheet ? "sheet" : "popover"} width={392} closeNow={open.sheet && !sheet} onClose={() => setOpen(null)}>
          {(dismiss) =>
            open.sheet ? (
              <>
                <div className="pulls-sheet-top" data-sheet-handle>
                  <span className="pulls-grab" aria-hidden />
                  <div className="pulls-sheet-head"><b>{issues.length === 1 ? "Issue" : `${issues.length} linked issues`}</b><IconButton label="Close" onClick={dismiss}><X size={14} aria-hidden /></IconButton></div>
                </div>
                <div className="pulls-sheet-body"><IssueList thread={thread} sheet onOpen={dismiss} /></div>
              </>
            ) : <IssueList thread={thread} onOpen={dismiss} />
          }
        </ToolbarPopover>
      ) : null}
    </>
  );
}
