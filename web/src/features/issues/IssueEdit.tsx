import { Pencil, Plus } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button } from "../../ui/controls";
import { issuesApi, markNotOffered } from "./api";
import type { IssueCandidates, IssueDetail } from "./contract";
import { ASSIGNEES_MAX, BODY_MAX, LABELS_MAX, byteLength, routeAbsent, updateProblem } from "./edit";
import { capText, reasonOf } from "./model";
import { PlannedControl } from "./Planned";

/**
 * The editors of an issue page: the title, the description, and the label and assignee pickers. They are the minimal inline
 * forms (no frame draws them yet). Every one of them follows the same rules:
 *
 * - The text is the tracker's, and what the person types goes back as a field value. It is shown only through React text and a
 *   controlled `textarea`, never as markup.
 * - One click is one write. `write` is the page's own guard: while one write is in flight a second is refused, so two clicks, or a
 *   description and a label write together, never run side by side. It answers the updated issue, which the page then shows.
 * - A failed write changes nothing on the page and loses nothing typed: the draft or the selection stays, and the form says why.
 * - A list over a limit is refused on this side (`updateProblem`) before anything is sent.
 * - A 404 or 405 that is not `not_found` means this service has no such route: the controls go back to Planned.
 */

/** The page's write guard: runs one write, answers the updated issue, and answers `null` without calling when one is already running. */
export type IssueWriter = (call: () => Promise<IssueDetail>) => Promise<IssueDetail | null>;

export interface EditProps {
  issue: IssueDetail;
  /** Whether the service has said it offers the writes. Otherwise the controls are the Planned pills. */
  offered: boolean;
  write: IssueWriter;
  writing: boolean;
}

/** What a failed write says, and whether it also means the route is not there. */
function failureWords(failure: unknown): string {
  if (failure instanceof ApiFailure && routeAbsent(failure.status, failure.code)) {
    markNotOffered("edit");
    return "This service does not offer editing yet. Nothing changed.";
  }
  return `${reasonOf(failure)} Nothing changed.`;
}

/** The description's pencil and, while it is open, its editor. Hands back the pencil for the header and the editor for the body. */
export function useDescriptionEditor({ issue, offered, write, writing }: EditProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const field = useId();
  const area = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { if (editing) area.current?.focus(); }, [editing]);
  const bytes = byteLength(draft);
  const problem = updateProblem({ body: draft });
  const unchanged = draft === issue.body;
  const save = async () => {
    if (writing || problem || unchanged) return;
    setError(null);
    try {
      const done = await write(() => issuesApi().update(issue, { body: draft }));
      if (done) setEditing(false);
    } catch (failure) {
      // The draft stays exactly as it was typed.
      setError(failureWords(failure));
    }
  };
  const pencil = !offered && !editing ? (
    <PlannedControl label="Edit description" what="Editing the description" className="iss-icb"><Pencil size={13} aria-hidden /></PlannedControl>
  ) : (
    <button type="button" className="iss-icb" aria-label="Edit description" aria-expanded={editing} disabled={editing || writing} onClick={() => { setDraft(issue.body); setError(null); setEditing(true); }}>
      <Pencil size={13} aria-hidden />
    </button>
  );
  const editor = editing ? (
    <form className="iss-edit" onSubmit={(event) => { event.preventDefault(); void save(); }}>
      <label htmlFor={field} className="iss-edit-label">Description</label>
      <textarea
        id={field}
        ref={area}
        rows={10}
        value={draft}
        disabled={writing}
        aria-invalid={problem !== null || undefined}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => { if (event.key === "Escape" && !writing) { event.stopPropagation(); setEditing(false); } }}
      />
      <div className="iss-edit-foot">
        <span className={problem ? "iss-edit-count iss-edit-over" : "iss-edit-count iss-quiet"}>{Math.ceil(bytes / 1024)} of {BODY_MAX / 1024} KiB</span>
        <Button small onClick={() => setEditing(false)} disabled={writing}>Cancel</Button>
        <Button small tone="primary" type="submit" busy={writing} disabled={problem !== null || unchanged}>Save</Button>
      </div>
      {problem ? <p className="iss-edit-error" role="alert">{problem}</p> : null}
      {error ? <p className="iss-edit-error" role="alert">{error}</p> : null}
    </form>
  ) : null;
  return { pencil, editor, editing };
}

/**
 * The issue's title in its page bar: the heading, and the pencil that edits it. While the form is open it takes the heading's
 * place in the bar, as the pull request page does; there is no separate card for it. It writes through the page's own guard,
 * so it shares the one-write-at-a-time rule with the description and the pickers, and a draft stays beside its refusal.
 * The panel tab stays read only: its comment says the tracker is where an issue is written.
 */
export function IssueTitle({ issue, offered, write, writing }: EditProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { if (editing) input.current?.focus(); }, [editing]);
  const problem = updateProblem({ title: draft });
  const unchanged = draft === issue.title;
  const save = async () => {
    if (writing || problem || unchanged) return;
    setError(null);
    try {
      const done = await write(() => issuesApi().update(issue, { title: draft }));
      if (done) setEditing(false);
    } catch (failure) {
      // The draft stays exactly as it was typed.
      setError(failureWords(failure));
    }
  };
  if (editing) {
    return (
      <form
        className="iss-title-form"
        onSubmit={(event) => { event.preventDefault(); void save(); }}
        onKeyDown={(event) => { if (event.key === "Escape" && !writing) { event.stopPropagation(); setEditing(false); } }}
      >
        <input
          ref={input}
          aria-label="Title"
          value={draft}
          disabled={writing}
          aria-invalid={problem !== null || undefined}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button small tone="primary" type="submit" busy={writing} disabled={problem !== null || unchanged}>Save</Button>
        <Button small onClick={() => setEditing(false)} disabled={writing}>Cancel</Button>
        {problem ? <p className="iss-edit-error" role="alert">{problem}</p> : null}
        {error ? <p className="iss-edit-error" role="alert">{error}</p> : null}
      </form>
    );
  }
  const pencil = !offered ? (
    <PlannedControl label="Edit title" what="Editing the title" className="iss-icb"><Pencil size={13} aria-hidden /></PlannedControl>
  ) : (
    <button type="button" className="iss-icb" aria-label="Edit title" aria-expanded={false} disabled={writing} onClick={() => { setDraft(issue.title); setError(null); setEditing(true); }}>
      <Pencil size={13} aria-hidden />
    </button>
  );
  return (
    <span className="iss-title-line">
      <h1 tabIndex={-1} data-page-title>{issue.title}</h1>
      {pencil}
    </span>
  );
}

type Kind = "labels" | "assignees";

interface Item { id: string; name: string; unresolved: boolean }

/** Every choice a picker lists: what the issue has now, first, then the candidates, each name once. Assignees keep the id a write needs. */
function itemsFor(kind: Kind, issue: IssueDetail, candidates: IssueCandidates | null): Item[] {
  const seen = new Set<string>();
  const items: Item[] = [];
  const add = (item: Item) => { if (!seen.has(item.id)) { seen.add(item.id); items.push(item); } };
  if (kind === "labels") {
    for (const label of issue.labels) add({ id: label.name, name: label.name, unresolved: false });
    for (const label of candidates?.labels ?? []) add({ id: label.name, name: label.name, unresolved: false });
    return items;
  }
  // A `Map`, not an object: a login comes from the tracker and could be `__proto__`.
  const ids = new Map((candidates?.assignees ?? []).map((who) => [who.login, who.id] as const));
  for (const who of issue.assignees) add({ id: who.login, name: who.login, unresolved: !ids.has(who.login) });
  for (const who of candidates?.assignees ?? []) add({ id: who.login, name: who.name ? `${who.login} · ${who.name}` : who.login, unresolved: false });
  return items;
}

/**
 * The plus button of the Labels or Assignees row and the picker it opens. The candidates are read when it opens, one read at a time
 * (a reopen drops the one still on its way), and the whole new set goes in one `update` when the person presses Apply.
 */
export function PickerEditor({ kind, issue, offered, write, writing }: EditProps & { kind: Kind }) {
  const [open, setOpen] = useState(false);
  const [read, setRead] = useState<{ status: "loading" } | { status: "error"; message: string } | { status: "ready"; data: IssueCandidates }>({ status: "loading" });
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  const group = useId();
  const noun = kind === "labels" ? "labels" : "assignees";
  const limit = kind === "labels" ? LABELS_MAX : ASSIGNEES_MAX;
  const current = kind === "labels" ? issue.labels.map((label) => label.name) : issue.assignees.map((who) => who.login);
  useEffect(() => () => request.current?.abort(), []);

  const load = () => {
    request.current?.abort();
    const asked = new AbortController();
    request.current = asked;
    setRead({ status: "loading" });
    issuesApi().candidates(issue, asked.signal).then(
      (data) => { if (!asked.signal.aborted) setRead({ status: "ready", data }); },
      (failure: unknown) => { if (!asked.signal.aborted) setRead({ status: "error", message: failureWords(failure).replace(/ Nothing changed\.$/, "") }); },
    );
  };
  const openIt = () => {
    setPicked(new Set(current));
    setFilter("");
    setError(null);
    setOpen(true);
    load();
  };
  const close = () => { request.current?.abort(); setOpen(false); };

  if (!offered && !open) {
    return <PlannedControl label={kind === "labels" ? "Change labels" : "Change assignees"} what={kind === "labels" ? "Changing labels" : "Changing assignees"}><Plus size={13} aria-hidden /></PlannedControl>;
  }
  const button = (
    <button type="button" className="iss-icb" aria-label={kind === "labels" ? "Change labels" : "Change assignees"} aria-expanded={open} disabled={open || writing} onClick={openIt}>
      <Plus size={13} aria-hidden />
    </button>
  );
  if (!open) return button;

  const candidates = read.status === "ready" ? read.data : null;
  const items = itemsFor(kind, issue, candidates);
  const wanted = filter.trim().toLowerCase();
  const shown = items.filter((item) => item.name.toLowerCase().includes(wanted)).slice(0, 250);
  const ids = new Map((candidates?.assignees ?? []).map((who) => [who.login, who.id] as const));
  const unresolved = items.filter((item) => item.unresolved && picked.has(item.id));
  const tooMany = picked.size > limit;
  const unchanged = picked.size === current.length && current.every((name) => picked.has(name));
  const problem = tooMany ? `An issue can have at most ${limit} ${noun}.` : unresolved.length > 0 ? `Orbyn cannot address ${unresolved.map((item) => capText(item.name, 40)).join(", ")} yet. Uncheck ${unresolved.length === 1 ? "it" : "them"} to continue.` : null;
  const toggle = (id: string) => setPicked((now) => { const next = new Set(now); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const apply = async () => {
    if (writing || problem || unchanged || read.status !== "ready") return;
    setError(null);
    const chosen = [...picked];
    const input = kind === "labels" ? { labels: chosen } : { assignees: chosen.map((login) => ids.get(login)).filter((id): id is string => id !== undefined) };
    // Over a limit is refused here, before anything is sent.
    const refused = updateProblem(input);
    if (refused) { setError(refused); return; }
    try {
      const done = await write(() => issuesApi().update(issue, input));
      if (done) setOpen(false);
    } catch (failure) {
      setError(failureWords(failure));
    }
  };
  return (
    <>
      {button}
      <form className="iss-edit iss-pick-form" role="group" aria-labelledby={group} onSubmit={(event) => { event.preventDefault(); void apply(); }} onKeyDown={(event) => { if (event.key === "Escape" && !writing) { event.stopPropagation(); close(); } }}>
        <b id={group}>{kind === "labels" ? "Change labels" : "Change assignees"}</b>
        {read.status === "loading" ? <p className="iss-quiet" role="status">Loading {noun}…</p> : null}
        {read.status === "error" ? (
          <div className="iss-start-fail" role="alert">
            <span className="iss-quiet">Couldn’t read the {noun}. {read.message}</span>
            <Button small onClick={load}>Retry</Button>
          </div>
        ) : null}
        {read.status === "ready" ? (
          <>
            <input type="search" aria-label={`Filter ${noun}`} placeholder={`Filter ${noun}`} value={filter} onChange={(event) => setFilter(event.target.value)} />
            <ul className="iss-pick-items">
              {shown.length === 0 ? <li className="iss-quiet">{items.length === 0 ? `No ${noun} to choose from.` : `No ${noun} match.`}</li> : shown.map((item) => (
                <li key={item.id}>
                  <label>
                    <input type="checkbox" checked={picked.has(item.id)} disabled={writing} onChange={() => toggle(item.id)} />
                    <span>{capText(item.name, 80)}</span>
                  </label>
                </li>
              ))}
            </ul>
            {read.data.truncated ? <p className="iss-quiet">The list is cut short. One that is missing may still exist.</p> : null}
          </>
        ) : null}
        {problem ? <p className="iss-edit-error" role="alert">{problem}</p> : null}
        {error ? <p className="iss-edit-error" role="alert">{error}</p> : null}
        <div className="iss-edit-foot">
          <Button small onClick={close} disabled={writing}>Cancel</Button>
          <Button small tone="primary" type="submit" busy={writing} disabled={read.status !== "ready" || problem !== null || unchanged}>Apply</Button>
        </div>
      </form>
    </>
  );
}
