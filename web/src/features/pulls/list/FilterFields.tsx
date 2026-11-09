import { X } from "lucide-react";
import { useState } from "react";
import { Segmented } from "../../../ui/Segmented";
import { Button, IconButton } from "../../../ui/controls";
import { clearedFilters, pullData, type PullFilters } from "../model/pullModel";
import type { ToolbarProps } from "./toolbarTypes";
import { Avatar, CHECK_OPTIONS, Matched, REVIEW_OPTIONS, STATE_OPTIONS, SearchBox, Tick, toggle } from "./toolbarParts";

export function FilterFields({ props, count, sheet, dismiss }: { props: ToolbarProps; count: number; sheet: boolean; dismiss(): void }) {
  const { filters, onFilters } = props;
  const has = props.has ?? pullData;
  const [authorFind, setAuthorFind] = useState("");
  const [labelFind, setLabelFind] = useState("");
  const set = (patch: Partial<PullFilters>) => onFilters({ ...filters, ...patch });
  const [active, setActive] = useState(0);
  const find = authorFind.trim();
  // Choosing someone adds the chip; the search and its list stay, with focus where it was.
  const pick = (name: string) => set({ author: filters.author === name ? null : name, me: false });
  const authors = find ? props.authors.filter((name) => name.toLowerCase().includes(find.toLowerCase())).slice(0, 6) : [];
  const labels = props.labels.filter((entry) => entry.name.toLowerCase().includes(labelFind.trim().toLowerCase()));
  const clearAll = () => {
    onFilters(clearedFilters(filters));
  };

  const state = (
    <>
      <div className="pulls-mh">State</div>
      {STATE_OPTIONS.map((entry) => (
        <Tick
          key={entry.value}
          on={filters.states.includes(entry.value)}
          label={entry.label}
          onClick={() => {
            const next = toggle(filters.states, entry.value);
            set({ states: next.length > 0 ? next : ["open"] });
          }}
        />
      ))}
    </>
  );
  const draft = (
    <>
      <div className="pulls-mh">Draft</div>
      <div className="pulls-msub">
        <Segmented
          label="Draft"
          value={filters.draft}
          options={[
            { value: "all", label: "Include" },
            { value: "hide", label: "Exclude" },
            { value: "only", label: "Only" },
          ]}
          onChange={(draftValue) => set({ draft: draftValue })}
        />
      </div>
    </>
  );
  const review = (
    <>
      <div className="pulls-mh">Review</div>
      {has.reviewRequests ? (
        <Tick
          on={filters.review.includes("review_required")}
          wait={props.detailsWait === "waiting"}
          label="Needs my review"
          onClick={() => set({ review: toggle(filters.review, "review_required") })}
        />
      ) : null}
      {REVIEW_OPTIONS.map((entry) => (
        <Tick key={entry.value} on={filters.review.includes(entry.value)} label={entry.label} onClick={() => set({ review: toggle(filters.review, entry.value) })} />
      ))}
    </>
  );
  const asked = (entry: string) => set({ requestedFrom: toggle(filters.requestedFrom, entry) });
  const requested = has.reviewRequests ? (
    <>
      <div className="pulls-mh">Review requested from</div>
      {props.viewer ? (
        <Tick
          on={filters.requestedFrom.includes(`user:${props.viewer}`)}
          wait={props.detailsWait === "waiting"}
          lead={<Avatar name={props.viewer} />}
          label="Me"
          onClick={() => asked(`user:${props.viewer}`)}
        />
      ) : null}
      {(props.requesters ?? []).length > 0 ? <div className="pulls-mh">People</div> : null}
      {(props.requesters ?? []).map((name) => (
        <Tick
          key={name}
          on={filters.requestedFrom.includes(`user:${name}`)}
          wait={props.detailsWait === "waiting"}
          lead={<Avatar name={name} />}
          label={name}
          onClick={() => asked(`user:${name}`)}
        />
      ))}
    </>
  ) : null;
  const checks = (
    <>
      <div className="pulls-mh">Checks</div>
      {CHECK_OPTIONS.map((entry) => (
        <Tick key={entry.value} on={filters.checks.includes(entry.value)} label={entry.label} onClick={() => set({ checks: toggle(filters.checks, entry.value) })} />
      ))}
    </>
  );
  const author = (
    <>
      <div className="pulls-mh">Author</div>
      <div className="pulls-msub">
        <Segmented
          label="Author"
          value={filters.me ? "me" : filters.author ? "named" : "anyone"}
          options={[
            ...(has.viewerLogin ? [{ value: "me" as const, label: "Me" }] : []),
            { value: "anyone" as const, label: "Anyone" },
          ]}
          onChange={(who) => set({ me: who === "me", author: null })}
        />
        <SearchBox
          label="Search people"
          placeholder="Search people"
          value={authorFind}
          onChange={(next) => {
            setAuthorFind(next);
            setActive(0);
          }}
          inputProps={{
            role: "combobox",
            "aria-expanded": authors.length > 0,
            "aria-controls": "pulls-author-list",
            "aria-activedescendant": authors[active] ? `pulls-author-${authors[active]}` : undefined,
            onKeyDown: (event) => {
              if (authors.length === 0) return;
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                setActive((at) => (at + (event.key === "ArrowDown" ? 1 : -1) + authors.length) % authors.length);
              } else if (event.key === "Enter") {
                event.preventDefault();
                const name = authors[active];
                if (name) pick(name);
              }
            },
          }}
        />
      </div>
      {authors.length > 0 ? (
        <div id="pulls-author-list" role="listbox" aria-label={`Authors matching ${find}`} className="pulls-people">
          {authors.map((name, at) => (
            <div
              key={name}
              id={`pulls-author-${name}`}
              role="option"
              aria-selected={at === active}
              className="pulls-prs"
              data-kf={at === active || undefined}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => pick(name)}
            >
              <span className="pulls-av" aria-hidden>
                {name.slice(0, 2).toUpperCase()}
              </span>
              <span className="pulls-pn">
                <Matched name={name} find={find} />
              </span>
              <span className="pulls-hd">
                @<Matched name={name} find={find} />
              </span>
            </div>
          ))}
        </div>
      ) : null}
      {find && authors.length === 0 ? <p className="pulls-none">No one matches “{find}”</p> : null}
      {filters.author ? (
        <div className="pulls-chipwrap">
          <span className="pulls-fchip">
            {filters.author}
            <button type="button" className="pulls-x" aria-label={`Remove author ${filters.author}`} onClick={() => set({ author: null })}>
              <X size={10} aria-hidden />
            </button>
          </span>
        </div>
      ) : null}
    </>
  );
  const label = (
    <>
      <div className="pulls-mh">Label</div>
      <div className="pulls-msub">
        <SearchBox label="Search labels" placeholder="Search labels" value={labelFind} onChange={setLabelFind} />
      </div>
      {labels.map((entry) => (
        <Tick
          key={entry.name}
          on={filters.labels.includes(entry.name)}
          label={entry.name}
          count={entry.count}
          onClick={() => set({ labels: toggle(filters.labels, entry.name) })}
        />
      ))}
    </>
  );

  if (sheet) {
    return (
      <>
        <div className="pulls-sheet-top" data-sheet-handle>
          <span className="pulls-grab" aria-hidden />
          <div className="pulls-sheet-head">
            <b>Filters</b>
            <IconButton label="Close filters" onClick={dismiss}>
              <X size={14} aria-hidden />
            </IconButton>
          </div>
        </div>
        <div className="pulls-sheet-body">
          {state}
          {draft}
          {review}
          {requested}
          {checks}
          {author}
          {label}
        </div>
        <div className="pulls-sheet-foot">
          <button type="button" className="pulls-ghost" onClick={clearAll}>
            Clear all
          </button>
          <Button tone="primary" className="pulls-show" onClick={dismiss}>
            Show {props.shown} pull {props.shown === 1 ? "request" : "requests"}
          </Button>
        </div>
      </>
    );
  }
  return (
    <>
      <div className="pulls-mcols">
        <div className="pulls-mc">
          {state}
          {draft}
          {review}
          {requested}
          {checks}
        </div>
        <div className="pulls-mc">
          {author}
          {label}
        </div>
      </div>
      <div className="pulls-mfoot">
        <button type="button" className="pulls-ghost" onClick={clearAll}>
          Clear all
        </button>
        <span className="pulls-mcount">{count} active</span>
      </div>
    </>
  );
}
