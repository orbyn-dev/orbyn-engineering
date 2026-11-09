import { Check, ChevronDown, Pause, Search, SlidersHorizontal, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Segmented } from "../../../ui/Segmented";
import { ToolbarPopover } from "./ToolbarPopover";
import "../pullsToolbar.css";
import { activeChips, clearedFilters, pullData, takeLabels, sortName, type PullSort } from "../model/pullModel";
import type { ToolbarProps } from "./toolbarTypes";
import type { Kind } from "./toolbarParts";
import { FilterFields } from "./FilterFields";

export function PullsToolbar(props: ToolbarProps) {
  const has = props.has ?? pullData;
  const { filters, onFilters, sort, onSort, text, onText, tokens, onTokens, onSubmit } = props;
  const [menu, setMenu] = useState<{ kind: Kind; anchor: HTMLElement; sheet: boolean } | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const chips = useMemo(() => activeChips(filters), [filters]);
  const count = chips.length;
  const project = props.projects.find((entry) => entry.id === filters.project);

  // "/" jumps to the search from anywhere that isn't already a text field.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true'], [role='dialog']")) return;
      event.preventDefault();
      field.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const commitTokens = (value: string, final: boolean, submit = false) => {
    const taken = takeLabels(value, final);
    if (taken.labels.length > 0) {
      const merged = [...new Set([...tokens, ...taken.labels])];
      if (merged.length !== tokens.length) onTokens(merged);
    }
    onText(taken.labels.length > 0 || final ? taken.rest : value);
    if (submit) onSubmit(taken.rest);
  };
  const clearSearch = () => {
    onText("");
    onTokens([]);
    onSubmit("");
  };
  const open = (kind: Kind) => (event: React.MouseEvent<HTMLElement>) => {
    const anchor = event.currentTarget;
    setMenu(menu?.kind === kind ? null : { kind, anchor, sheet: props.phone && kind === "filters" });
  };
  const sheet = props.phone;

  const dots = project ? [project] : props.projects.slice(0, 3);
  const searchField = (
    <div className="pulls-fld" data-focus-scope>
      <div className="pulls-inp">
        <Search size={14} aria-hidden />
        {tokens.map((token) => (
          <span key={token} className="pulls-tok">
            label:{token}
            <button type="button" className="pulls-x" aria-label={`Remove label:${token}`} onClick={() => onTokens(tokens.filter((one) => one !== token))}>
              <X size={10} aria-hidden />
            </button>
          </span>
        ))}
        <input
          ref={field}
          aria-label="Search pull requests"
          placeholder={tokens.length > 0 ? "" : "Search, or label:bug"}
          value={text}
          onChange={(event) => {
            commitTokens(event.target.value, false);
          }}
          onBlur={() => commitTokens(text, true)}
          onKeyDown={(event) => {
            if (event.key === "Enter") commitTokens(text, true, true);
            else if (event.key === "Escape" && (text || tokens.length > 0)) {
              event.stopPropagation();
              clearSearch();
            } else if (event.key === "Backspace" && !text && tokens.length > 0) onTokens(tokens.slice(0, -1));
          }}
        />
        {text || tokens.length > 0 ? (
          <button type="button" className="pulls-x pulls-clear" aria-label="Clear search" onClick={() => { clearSearch(); field.current?.focus(); }}>
            <X size={12} aria-hidden />
          </button>
        ) : (
          <kbd aria-hidden>/</kbd>
        )}
      </div>
    </div>
  );

  return (
    <>
      <div className="pulls-tb">
        <div className="pulls-tbp" role="group" aria-label="Quick filters">
          <button
            type="button"
            className="pulls-pill"
            aria-pressed={filters.states.length === 1 && filters.states[0] === "open" && filters.draft === "all"}
            onClick={() => onFilters({ ...filters, states: ["open"], draft: "all" })}
          >
            Open · {props.loading ? "\u2026" : props.unavailable ? "\u2013" : props.openCount}
          </button>
          <button
            type="button"
            className="pulls-pill"
            aria-pressed={filters.states.length === 1 && filters.states[0] === "open" && filters.draft === "only"}
            onClick={() => onFilters({ ...filters, states: ["open"], draft: "only" })}
          >
            Drafts · {props.loading ? "\u2026" : props.unavailable ? "\u2013" : props.draftCount}
          </button>
          {(["merged", "closed"] as const).map((state) => (
            <button
              key={state}
              type="button"
              className="pulls-pill"
              aria-pressed={filters.states.length === 1 && filters.states[0] === state && filters.draft === "all"}
              onClick={() => onFilters({ ...filters, states: [state], draft: "all" })}
            >
              {state === "merged" ? "Merged" : "Closed"}
            </button>
          ))}
        </div>
        <div className="pulls-tbr">
          <div className="pulls-searchrow">
            {searchField}
            {props.phoneRefresh}
          </div>
          <div className="pulls-sels">
            <button type="button" className="pulls-sel" aria-haspopup="menu" aria-expanded={menu?.kind === "project"} data-on={menu?.kind === "project" || undefined} onClick={open("project")}>
              <span className="pulls-dots" aria-hidden>
                {dots.map((entry) => (
                  <i key={entry.id} className={`project-color-${entry.color}`} />
                ))}
              </span>
              {project ? project.name : "All projects"}
              <ChevronDown size={14} aria-hidden />
            </button>
            <button type="button" className="pulls-sel" aria-haspopup="dialog" aria-expanded={menu?.kind === "filters"} data-on={menu?.kind === "filters" || count > 0 || undefined} onClick={open("filters")}>
              <SlidersHorizontal size={14} aria-hidden />
              {count > 0 ? `Filters · ${count}` : "Filters"}
            </button>
            <button type="button" className="pulls-sel" aria-haspopup="menu" aria-expanded={menu?.kind === "sort"} data-on={menu?.kind === "sort" || undefined} onClick={open("sort")}>
              Sort: {sortName(sort)}
              <ChevronDown size={14} aria-hidden />
            </button>
          </div>
        </div>
      </div>
      {chips.length > 0 ? (
        <div className="pulls-chiprow" role="group" aria-label="Active filters">
          {chips.map((chip) => {
            const dependent = props.detailsWait !== undefined && (chip.id === "review:review_required" || chip.id.startsWith("requested:"));
            const waiting = dependent && props.detailsWait === "waiting";
            const chipEl = (
              <span key={chip.id} className="pulls-fchip" data-wait={dependent ? props.detailsWait : undefined}>
                {dependent && props.detailsWait === "failed" ? <TriangleAlert size={14} className="pulls-chip-alert" aria-hidden /> : null}
                {dependent && props.detailsWait === "paused" ? <Pause size={14} className="pulls-chip-alert" aria-hidden /> : null}
                {chip.label}
                <button type="button" className="pulls-x" aria-label={`Remove filter ${chip.label}`} onClick={() => onFilters(chip.remove(filters))}>
                  {waiting ? <i className="pulls-wait spin" aria-hidden /> : <X size={10} aria-hidden />}
                </button>
              </span>
            );
            // A paused chip says why in the tooltip the rows use, not the browser's.
            return dependent && props.detailsWait === "paused" ? (
              <span key={chip.id} className="pulls-chipwrap-tip">
                {chipEl}
                <span role="tooltip" className="pull-tip">
                  Paused
                </span>
              </span>
            ) : (
              chipEl
            );
          })}
          <button type="button" className="pulls-ghost" onClick={() => onFilters(clearedFilters(filters))}>
            Clear all
          </button>
        </div>
      ) : null}
      {menu?.kind === "project" ? (
        <ToolbarPopover anchor={menu.anchor} label="Project" role="menu" width={240} onClose={() => setMenu(null)}>
          {(dismiss) => (
            <>
              <div className="pulls-mh">Project</div>
              <button
                type="button"
                role="menuitemradio"
                aria-checked={filters.project === null}
                className="pulls-mi"
                onClick={() => {
                  onFilters({ ...filters, project: null });
                  dismiss();
                }}
              >
                <span className="pulls-lead">{filters.project === null ? <Check size={14} aria-hidden /> : null}</span>
                <i className="pulls-dot pulls-dot-hollow" aria-hidden />
                All projects
                <span className="pulls-k">{props.allCount}</span>
              </button>
              {props.projects.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={filters.project === entry.id}
                  className="pulls-mi"
                  onClick={() => {
                    onFilters({ ...filters, project: entry.id });
                    dismiss();
                  }}
                >
                  <span className="pulls-lead">{filters.project === entry.id ? <Check size={14} aria-hidden /> : null}</span>
                  <i className={`pulls-dot project-color-${entry.color}`} aria-hidden />
                  {entry.name}
                  <span className="pulls-k">{entry.count}</span>
                </button>
              ))}
            </>
          )}
        </ToolbarPopover>
      ) : null}
      {menu?.kind === "sort" ? (
        <ToolbarPopover anchor={menu.anchor} label="Sort" role="menu" width={200} onClose={() => setMenu(null)}>
          {(dismiss) => (
            <>
              <div className="pulls-mh">Sort by</div>
              {(
                [
                  { label: "Updated", on: sort.by === "updated", to: { by: "updated", dir: sort.dir }, shown: true },
                  { label: "Created", on: sort.by === "created" && sort.dir === "newest", to: { by: "created", dir: "newest" }, shown: props.oneRepo },
                  { label: "Most comments", on: sort.by === "comments", to: { by: "comments", dir: "newest" }, shown: has.commentCount },
                  { label: "Oldest", on: sort.by === "created" && sort.dir === "oldest", to: { by: "created", dir: "oldest" }, shown: props.oneRepo },
                ] as { label: string; on: boolean; to: PullSort; shown: boolean }[]
              )
                .filter((entry) => entry.shown)
                .map((entry) => (
                  <button
                    key={entry.label}
                    type="button"
                    role="menuitemradio"
                    aria-checked={entry.on}
                    className="pulls-mi"
                    onClick={() => {
                      onSort(entry.to);
                      dismiss();
                    }}
                  >
                    <span className="pulls-lead">{entry.on ? <Check size={14} aria-hidden /> : null}</span>
                    {entry.label}
                  </button>
                ))}
              <hr className="pulls-msep" />
              <div className="pulls-mh">Direction</div>
              <div className="pulls-msub">
                <Segmented
                  label="Direction"
                  value={sort.dir}
                  options={[
                    { value: "newest", label: "Newest first" },
                    { value: "oldest", label: "Oldest first" },
                  ]}
                  onChange={(dir) => onSort({ ...sort, dir })}
                />
              </div>
            </>
          )}
        </ToolbarPopover>
      ) : null}
      {menu?.kind === "filters" ? (
        <ToolbarPopover
          anchor={menu.anchor}
          label="Filters"
          layout={menu.sheet ? "sheet" : "popover"}
          width={470}
          // Past the phone breakpoint the sheet closes the normal way; it never turns into the popover.
          closeNow={menu.sheet && !sheet}
          onClose={() => setMenu(null)}
        >
          {(dismiss) => <FilterFields props={props} count={count} sheet={menu.sheet} dismiss={dismiss} />}
        </ToolbarPopover>
      ) : null}
    </>
  );
}
