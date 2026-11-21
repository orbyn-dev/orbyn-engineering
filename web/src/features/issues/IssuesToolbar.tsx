import { Check, ChevronDown, ListFilter, Search, X } from "lucide-react";
import { useRef, useState, type FormEvent } from "react";
import { navigate } from "../../product/state/route";
import { Button, IconButton } from "../../ui/controls";
import { ToolbarPopover } from "../pulls";
import { TRACKER_NAMES, type IssueQuery, type TrackerKind, type TrackerRow } from "./contract";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { useSortOffered } from "./api";
import { PlannedControl } from "./Planned";
import { activeChips, noQuery, parseSearch, shownLine, sortWord } from "./model";

const STATES = [["open", "Open"], ["closed", "Closed"], ["all", "All"]] as const;
const ASSIGNEES = [["any", "Anyone"], ["me", "Assigned to me"], ["none", "Unassigned"]] as const;

export interface ToolbarProps {
  query: IssueQuery;
  onQuery(next: IssueQuery): void;
  trackers: TrackerRow[];
  counts: { open: number; closed: number } | null;
  labels: { name: string; count: number }[];
  shown: number | null;
  phone: boolean;
  loading: boolean;
}

const badge = (tracker: TrackerKind | null) => (tracker === null ? "All" : TRACKER_NAMES[tracker].slice(0, 2).toUpperCase());
const countText = (value: number | undefined) => (value === undefined ? "–" : String(value));

function TrackerGroup({ query, trackers, onPick, dismiss }: { query: IssueQuery; trackers: TrackerRow[]; onPick(tracker: TrackerKind | null): void; dismiss?(): void }) {
  const connected = trackers.filter((row) => row.connected).length;
  return (
    <div className="iss-pop-list" role="group" aria-label="Tracker">
      <button type="button" className="iss-pick" aria-pressed={query.tracker === null} onClick={() => { onPick(null); dismiss?.(); }}>
        <span className="iss-badge" aria-hidden>All</span>
        <span className="iss-pick-name"><b>All trackers</b><span>{connected} connected</span></span>
        {query.tracker === null ? <Check size={14} aria-hidden /> : null}
      </button>
      {trackers.map((row) => (
        <div key={row.tracker} className="iss-pick-row">
          <button type="button" className="iss-pick" aria-pressed={query.tracker === row.tracker} onClick={() => { onPick(row.tracker); dismiss?.(); }}>
            <span className="iss-badge" aria-hidden>{badge(row.tracker)}</span>
            <span className="iss-pick-name"><b>{TRACKER_NAMES[row.tracker]}</b><span>{row.connected ? `${row.scope ?? ""}${row.scope ? " · " : ""}Connected` : "Not connected"}</span></span>
            {query.tracker === row.tracker ? <Check size={14} aria-hidden /> : null}
          </button>
          {row.connected ? null : (
            <Button small onClick={() => { navigate({ name: "settings", section: "source-control" }); dismiss?.(); }}>Connect</Button>
          )}
        </div>
      ))}
      <button type="button" className="iss-pop-foot" onClick={() => navigate({ name: "settings", section: "source-control" })}>Manage trackers in Settings</button>
    </div>
  );
}

/** State, labels and assignee: the only filters. On a phone the tracker joins them in the one sheet. */
function FilterPanel({ query, trackers, labels, phone, onApply, dismiss }: { query: IssueQuery; trackers: TrackerRow[]; labels: ToolbarProps["labels"]; phone: boolean; onApply(next: IssueQuery): void; dismiss(): void }) {
  const [draft, setDraft] = useState(query);
  const toggle = (name: string) => setDraft({ ...draft, labels: draft.labels.includes(name) ? draft.labels.filter((one) => one !== name) : [...draft.labels, name] });
  const known = [...new Set([...labels.map((label) => label.name), ...draft.labels])];
  return (
    <form className="iss-panel" onSubmit={(event: FormEvent) => { event.preventDefault(); onApply(draft); dismiss(); }}>
      {phone ? <TrackerGroup query={draft} trackers={trackers} onPick={(tracker) => setDraft({ ...draft, tracker })} /> : null}
      <div role="group" aria-label="State" className="iss-group">
        <b>State</b>
        {STATES.map(([id, word]) => (
          <label key={id}><input type="radio" name="iss-state" checked={draft.state === id} onChange={() => setDraft({ ...draft, state: id })} />{word}</label>
        ))}
      </div>
      <div role="group" aria-label="Labels" className="iss-group">
        <b>Labels</b>
        {known.length === 0 ? <span className="iss-quiet">No labels in this list</span> : known.map((name) => (
          <label key={name}>
            <input type="checkbox" checked={draft.labels.includes(name)} onChange={() => toggle(name)} />
            {name}
            <span className="iss-quiet">{labels.find((label) => label.name === name)?.count ?? ""}</span>
          </label>
        ))}
      </div>
      <div role="group" aria-label="Assignee" className="iss-group">
        <b>Assignee</b>
        {ASSIGNEES.map(([id, word]) => (
          <label key={id}><input type="radio" name="iss-assignee" checked={draft.assignee === id} onChange={() => setDraft({ ...draft, assignee: id })} />{word}</label>
        ))}
      </div>
      <div className="iss-panel-foot">
        <Button small tone="quiet" onClick={() => setDraft({ ...noQuery, tracker: draft.tracker, text: draft.text, sort: draft.sort })}>Clear</Button>
        <Button small tone="primary" type="submit">Apply</Button>
      </div>
    </form>
  );
}

export function IssuesToolbar(props: ToolbarProps) {
  const { query, onQuery, trackers, counts, phone } = props;
  const [menu, setMenu] = useState<{ kind: "tracker" | "filters"; anchor: HTMLElement } | null>(null);
  const [text, setText] = useState(query.text);
  const sortLive = useSortOffered();
  const [sortMenu, setSortMenu] = useState<{ x: number; y: number } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const chips = activeChips(query);
  const filterCount = query.labels.length + (query.assignee === "any" ? 0 : 1);
  const toggle = (kind: "tracker" | "filters", anchor: HTMLElement) => setMenu(menu?.kind === kind ? null : { kind, anchor });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseSearch(text);
    setText(parsed.text);
    onQuery({ ...query, text: parsed.text, labels: [...new Set([...query.labels, ...parsed.labels])] });
  };
  const remove = (id: string) => {
    if (id === "text") { setText(""); onQuery({ ...query, text: "" }); }
    else if (id === "assignee") onQuery({ ...query, assignee: "any" });
    else onQuery({ ...query, labels: query.labels.filter((label) => `label:${label}` !== id) });
  };
  const clear = () => { setText(""); onQuery({ ...noQuery, tracker: query.tracker, state: query.state, sort: query.sort }); };
  const trackerLabel = query.tracker === null ? "All trackers" : TRACKER_NAMES[query.tracker];
  const sheet = phone && menu?.kind === "filters";
  return (
    <div className="iss-toolbar">
      <div className="iss-tabs" role="group" aria-label="State">
        {STATES.map(([id, word]) => (
          <button key={id} type="button" aria-pressed={query.state === id} onClick={() => onQuery({ ...query, state: id })}>
            {id === "all" ? word : `${word} · ${countText(counts?.[id])}`}
          </button>
        ))}
      </div>
      <form className="iss-search" role="search" onSubmit={submit}>
        <Search size={14} aria-hidden />
        <input ref={input} type="search" aria-label="Search issues" placeholder="Search, or label:bug" value={text} onChange={(event) => setText(event.target.value)} />
        {phone ? null : <kbd aria-hidden>/</kbd>}
      </form>
      {phone ? null : (
        <button type="button" className="iss-ctl" aria-haspopup="dialog" aria-expanded={menu?.kind === "tracker"} aria-label={`Tracker: ${trackerLabel}`} onClick={(event) => toggle("tracker", event.currentTarget)}>
          <span className="iss-badge" aria-hidden>{badge(query.tracker)}</span>{trackerLabel}<ChevronDown size={12} aria-hidden />
        </button>
      )}
      <button type="button" className="iss-ctl" aria-haspopup="dialog" aria-expanded={menu?.kind === "filters"} onClick={(event) => toggle("filters", event.currentTarget)}>
        <ListFilter size={14} aria-hidden />{filterCount > 0 ? `Filters · ${filterCount}` : "Filters"}
      </button>
      {phone ? null : sortLive ? (
        // Live only once the service has said it offers the writes and the sort (`useSortOffered`); the choice is the query's `sort`.
        <>
          <button
            type="button"
            className="iss-ctl"
            aria-haspopup="menu"
            aria-expanded={sortMenu !== null}
            aria-label={`Sort: ${sortWord(query.sort)}`}
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setSortMenu(sortMenu ? null : { x: rect.left, y: rect.bottom + 4 });
            }}
          >
            Sort: {sortWord(query.sort)}<ChevronDown size={12} aria-hidden />
          </button>
          <Presence>
            {sortMenu ? (
              <Menu
                x={sortMenu.x}
                y={sortMenu.y}
                label="Sort issues"
                minWidth={160}
                onClose={() => setSortMenu(null)}
                items={(["updated", "created"] as const).map((id) => ({
                  label: sortWord(id),
                  selected: (query.sort ?? "updated") === id,
                  leadCheck: true,
                  onSelect: () => { if ((query.sort ?? "updated") !== id) onQuery({ ...query, sort: id }); },
                }))}
              />
            ) : null}
          </Presence>
        </>
      ) : (
        // Planned until the service offers it: the list is newest update first and nothing else is on offer yet.
        <PlannedControl label="Sort: Updated" what="Sorting" className="iss-ctl">Sort: Updated<ChevronDown size={12} aria-hidden /></PlannedControl>
      )}
      {chips.length > 0 ? (
        <div className="iss-chips">
          {chips.map((chip) => (
            <span key={chip.id} className="iss-chip">{chip.label}<button type="button" aria-label={`Remove ${chip.label}`} onClick={() => remove(chip.id)}><X size={11} aria-hidden /></button></span>
          ))}
          {props.shown !== null && counts ? <span className="iss-count" role="status">{shownLine(props.shown, counts, query.state)}</span> : null}
          <button type="button" className="iss-clear" onClick={clear}>Clear filters</button>
        </div>
      ) : null}
      {menu ? (
        <ToolbarPopover anchor={menu.anchor} label={menu.kind === "tracker" ? "Tracker" : "Filters"} layout={sheet ? "sheet" : "popover"} width={menu.kind === "tracker" ? 320 : 280} onClose={() => setMenu(null)}>
          {(dismiss) =>
            menu.kind === "tracker" ? (
              <TrackerGroup query={query} trackers={trackers} onPick={(tracker) => onQuery({ ...query, tracker })} dismiss={dismiss} />
            ) : sheet ? (
              <>
                <div className="pulls-sheet-top" data-sheet-handle>
                  <span className="pulls-grab" aria-hidden />
                  <div className="pulls-sheet-head"><b>Filters</b><IconButton label="Close" onClick={dismiss}><X size={14} aria-hidden /></IconButton></div>
                </div>
                <div className="pulls-sheet-body"><FilterPanel query={query} trackers={trackers} labels={props.labels} phone onApply={onQuery} dismiss={dismiss} /></div>
              </>
            ) : (
              <FilterPanel query={query} trackers={trackers} labels={props.labels} phone={false} onApply={onQuery} dismiss={dismiss} />
            )
          }
        </ToolbarPopover>
      ) : null}
    </div>
  );
}
