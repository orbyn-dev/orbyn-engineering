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
 * The editors of an issue page (issue-write-description, -description-refused, -labels, -assignees, -not-offered): the description, and
 * the label and assignee pickers the create dialog shares. Every one of them follows the same rules:
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

type Kind = "labels" | "assignees";

/** One row of a picker: the key the list checks it by, the words it shows, and an assignee's login when the row is a person. */
interface Item {
  /** A label's name, or an assignee's login. */
  id: string;
  name: string;
  /** An assignee's login, when the row is a person. */
  login?: string;
}

/** What a picker hands back on Apply: the labels' names, or the assignees with the id their write sends. */
export interface PickChoice {
  labels?: string[];
  assignees?: { login: string; id: string }[];
}

/** Every choice a picker lists: what is already chosen first, then the candidates, each name once. */
function itemsFor(kind: Kind, mine: { labels: string[]; assignees: { login: string }[] }, candidates: IssueCandidates | null): Item[] {
  const seen = new Set<string>();
  const items: Item[] = [];
  const add = (item: Item) => { if (!seen.has(item.id)) { seen.add(item.id); items.push(item); } };
  if (kind === "labels") {
    for (const name of mine.labels) add({ id: name, name });
    for (const label of candidates?.labels ?? []) add({ id: label.name, name: label.name });
    return items;
  }
  for (const who of mine.assignees) add({ id: who.login, name: who.login, login: who.login });
  for (const who of candidates?.assignees ?? []) add({ id: who.login, name: who.name ? `${who.login} · ${who.name}` : who.login, login: who.login });
  return items;
}

interface PickerProps {
  kind: Kind;
  /** The tracker and scope the candidates are read for: an issue, or the scope a new issue goes into. */
  target: Pick<IssueDetail, "tracker" | "scope">;
  /** The items chosen now, by their key (a label's name, an assignee's login). */
  current: string[];
  /** What is already chosen, drawn first and checked: the issue's own labels and people, or nothing for a new issue. */
  mine: { labels: string[]; assignees: { login: string }[] };
  /** Whether the service offers the writes; otherwise the plus is the Planned pill. */
  offered: boolean;
  /** A write in flight: the picker's own controls are off. */
  busy: boolean;
  /** The plus's accessible name and the picker's heading, in the frame's words ("Change labels", "Choose labels"). */
  label: string;
  heading: string;
  /** Called with the whole new set on Apply; a rejection is shown as the page does and keeps the picker open. */
  save(choice: PickChoice): Promise<boolean> | boolean;
}

/**
 * The plus of a Labels or Assignees row and the picker it opens. The candidates are read when it opens, one read at a time (a
 * reopen drops the one still on its way), and the whole new set goes to `save` on Apply. The issue page writes it and the
 * new-issue dialog holds it, so the reading, filtering, limits and refusals live in one place.
 */
export function Picker({ kind, target, current, mine, offered, busy, label, heading, save }: PickerProps) {
  const [open, setOpen] = useState(false);
  const [read, setRead] = useState<{ status: "loading" } | { status: "error"; message: string } | { status: "ready"; data: IssueCandidates }>({ status: "loading" });
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  const group = useId();
  const noun = kind === "labels" ? "labels" : "assignees";
  const limit = kind === "labels" ? LABELS_MAX : ASSIGNEES_MAX;
  useEffect(() => () => request.current?.abort(), []);

  const load = () => {
    request.current?.abort();
    const asked = new AbortController();
    request.current = asked;
    setRead({ status: "loading" });
    issuesApi().candidates(target, asked.signal).then(
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
    return <PlannedControl label={label} what={kind === "labels" ? "Changing labels" : "Changing assignees"}><Plus size={13} aria-hidden /></PlannedControl>;
  }
  const button = (
    <button type="button" className="iss-icb" aria-label={label} aria-expanded={open} disabled={open || busy} onClick={openIt}>
      <Plus size={13} aria-hidden />
    </button>
  );
  if (!open) return button;

  const candidates = read.status === "ready" ? read.data : null;
  const items = itemsFor(kind, mine, candidates);
  const wanted = filter.trim().toLowerCase();
  const shown = items.filter((item) => item.name.toLowerCase().includes(wanted)).slice(0, 250);
  // A `Map`, not an object: a login comes from the tracker and could be `__proto__`.
  const ids = new Map((candidates?.assignees ?? []).map((who) => [who.login, who.id] as const));
  const chosen = items.filter((item) => picked.has(item.id));
  const tooMany = picked.size > limit;
  const unchanged = picked.size === current.length && current.every((id) => picked.has(id));
  const problem = tooMany ? `An issue can have at most ${limit} ${noun}.` : null;
  const toggle = (id: string) => setPicked((now) => { const next = new Set(now); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const apply = async () => {
    if (busy || problem || unchanged || read.status !== "ready") return;
    setError(null);
    const choice: PickChoice = kind === "labels"
      ? { labels: chosen.map((item) => item.name) }
      // A person goes out by the candidate's id, never by a login typed by hand. A current assignee the candidates do not
      // carry has no id here; the candidates are built for GitHub only, and GitHub addresses a person by login, so the
      // issue's own login is what the write sends for her (docs/issues.md, "As built, GitHub").
      : { assignees: chosen.flatMap((item) => item.login === undefined ? [] : [{ login: item.login, id: ids.get(item.login) ?? item.login }]) };
    try {
      if (await save(choice)) setOpen(false);
    } catch (failure) {
      setError(failureWords(failure));
    }
  };
  return (
    <>
      {button}
      <form className="iss-edit iss-pick-form" role="group" aria-labelledby={group} onSubmit={(event) => { event.preventDefault(); void apply(); }} onKeyDown={(event) => { if (event.key === "Escape" && !busy) { event.stopPropagation(); close(); } }}>
        <b id={group}>{heading}</b>
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
                    <input type="checkbox" checked={picked.has(item.id)} disabled={busy} onChange={() => toggle(item.id)} />
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
          <Button small onClick={close} disabled={busy}>Cancel</Button>
          <Button small tone="primary" type="submit" busy={busy} disabled={read.status !== "ready" || problem !== null || unchanged}>Apply</Button>
        </div>
      </form>
    </>
  );
}

/** The Labels or Assignees plus of an issue page: the whole new set goes in one `POST /api/issues/item/update`. */
export function PickerEditor({ kind, issue, offered, write, writing }: EditProps & { kind: Kind }) {
  return (
    <Picker
      kind={kind}
      target={issue}
      current={kind === "labels" ? issue.labels.map((one) => one.name) : issue.assignees.map((who) => who.login)}
      mine={{ labels: issue.labels.map((one) => one.name), assignees: issue.assignees.map((who) => ({ login: who.login })) }}
      offered={offered}
      busy={writing}
      label={kind === "labels" ? "Change labels" : "Change assignees"}
      heading={kind === "labels" ? "Change labels" : "Change assignees"}
      save={async (choice) => {
        const input = kind === "labels" ? { labels: choice.labels ?? [] } : { assignees: (choice.assignees ?? []).map((who) => who.id) };
        return (await write(() => issuesApi().update(issue, input))) !== null;
      }}
    />
  );
}
