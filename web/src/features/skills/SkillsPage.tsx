import { Book, Search, X } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import type { SkillSummary } from "../../generated/SkillSummary";
import { useWaited } from "../../lib/motion";
import { useLayoutWidth } from "../../lib/useLayout";
import { DRAFT_KEY } from "../../product/state/draftStore";
import { navigate, routeHref } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { Button } from "../../ui/controls";
import { ErrorState, NarrowBack } from "../../product/ui/states";
import { useSkills } from "./skillsStore";
import { useCrewSkills, type AddedSkill, type ProposedSkill } from "./crewSkills";
import { ADDED_EMPTY_LINE, ADDED_GROUP_NOTE, AVAILABLE, PROPOSED_GROUP_NOTE, lessonsWord, skillText } from "../crew";
import "./skills.css";

/** The categories the library itself defines, in the order its catalog lists them. A skill with none shows only under All. */
export const categoriesOf = (skills: readonly SkillSummary[]): string[] => [
  ...new Set(skills.map((skill) => skill.category.trim()).filter((category) => category !== "")),
];

function startThreadWithSkill(skill: SkillSummary) {
  const { drafts, setDraft, draftProject } = useUi.getState();
  const draft = drafts[DRAFT_KEY] ?? "";
  const separator = draft && !/\s$/.test(draft) ? " " : "";
  setDraft(DRAFT_KEY, `${draft}${separator}$${skill.name} `);
  navigate({ name: "new", projectId: draftProject });
}

export function SkillsPage({ skill }: { skill?: string }) {
  const result = useSkills();
  // Crew's skill proposals and the skills approved from them (KC9): nothing is read while Crew Preview is off, and a
  // service without the routes answers 404, so the page then draws exactly what it drew before.
  const crew = useCrewSkills();
  const phone = useLayoutWidth() === "phone";
  const slow = useWaited(result.loading && !result.data);
  const [category, setCategory] = useState<string>("All");
  const [search, setSearch] = useState("");
  const heading = useRef<HTMLHeadingElement>(null);
  /** The card that was pressed, so going back lands on it. */
  const opened = useRef<string | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const listScrollTop = useRef(0);
  const entries = result.data?.skills;
  const personal = crew.status === "ready" ? crew.added.find((entry) => entry.name === skill) : undefined;
  const known = entries?.some((entry) => entry.name === skill) || personal !== undefined;
  // A skill added from lessons is known only once its read has answered: the address is not stale while that read runs.
  const crewSettled = crew.status !== "loading";
  // A stale bookmark must not remain behind the catalog in the Back trail.
  useEffect(() => {
    if (!entries || skill === undefined || known || !crewSettled) return;
    window.history.replaceState(window.history.state, "", routeHref({ name: "skills" }));
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  }, [entries, skill, known, crewSettled]);
  // The shell moves focus into every new page after its own effects run, and prefers an element marked
  // data-autofocus: the card that was opened is marked, so the shell and this page agree on where focus lands.
  // Focus follows the route, once the library is there: into a skill's heading, or back to the card that was opened.
  // It does not depend on the library's data itself: a later read of the same library (a slow workspace read changes its
  // revision) must not pull focus away from where the person is.
  const ready = entries !== undefined;
  useEffect(() => {
    if (!ready) return;
    if (skill !== undefined) {
      if (!known) return;
      heading.current?.focus();
      return;
    }
    const card = opened.current ? grid.current?.querySelector<HTMLElement>(`[data-skill="${CSS.escape(opened.current)}"]`) : null;
    (card ?? heading.current)?.focus({ preventScroll: true });
    if (opened.current && content.current) {
      const scroller = content.current;
      scroller.scrollTop = listScrollTop.current;
      if (card) {
        const bounds = scroller.getBoundingClientRect();
        const target = card.getBoundingClientRect();
        if (target.top < bounds.top) scroller.scrollTop += target.top - bounds.top;
        else if (target.bottom > bounds.bottom) scroller.scrollTop += target.bottom - bounds.bottom;
      }
    }
  }, [skill, ready, known]);

  // The phone has no persistent sidebar: keep a way out even while the catalog cannot load.
  if (phone && !result.data) {
    return <section className="skills-page" aria-label={skill === undefined ? "Skills" : "Skill"}>
      <header className="skills-heading">
        <NarrowBack to={skill === undefined ? undefined : { name: "skills" }} />
        <h1 tabIndex={-1} data-page-title>{skill ?? "Skills"}</h1>
      </header>
      {result.error ? <ErrorState title="Couldn't load skills" failure={result.error} onRetry={() => void result.reload()} />
        : slow ? <div className="skills-loading" role="status" aria-label="Loading skills">
          {Array.from({ length: 6 }, (_, index) => <div className="skills-placeholder" key={index} />)}
        </div> : null}
    </section>;
  }
  if (!result.data && !result.error) {
    return slow ? (
      <div className="skills-loading" role="status" aria-label="Loading skills">
        {Array.from({ length: 6 }, (_, index) => (
          <div className="skills-placeholder" key={index} />
        ))}
      </div>
    ) : null;
  }
  if (!result.data && result.error) {
    return <ErrorState title="Couldn't load skills" failure={result.error} onRetry={() => void result.reload()} />;
  }
  if (!result.data || !entries) return null;
  const selected = skill === undefined ? undefined : entries.find((entry) => entry.name === skill);
  const query = search.trim().toLowerCase();
  const shown = entries.filter(
    (entry) => (category === "All" || entry.category.trim() === category) && (!query || `${entry.name} ${entry.description}`.toLowerCase().includes(query)),
  );
  const categories = categoriesOf(entries);
  // The Crew groups are drawn on the page a person lands on, as the design draws them: no filter and no search.
  const crewReady = crew.status === "ready";
  const showCrew = crewReady && category === "All" && query === "";
  const crewParts =
    crewReady && showCrew
      ? [
          crew.proposed.length > 0 ? `${crew.proposed.length} proposed` : "",
          crew.added.length > 0 ? `${crew.added.length} added from lessons` : "",
        ].filter(Boolean)
      : [];

  if (skill !== undefined) {
    if (!selected) {
      if (personal) return <PersonalSkill skill={personal} heading={heading} />;
      return null;
    }
    const label = selected.category.trim();
    return (
      <section className="skills-page" aria-label="Skill">
        <header className="skills-heading">
          <NarrowBack to={{ name: "skills" }} />
          <h1 ref={heading} tabIndex={-1} data-page-title>
            {selected.name}
          </h1>
          <span className="skills-subtitle">{label ? `Built in · ${label}` : "Built in"}</span>
          {selected.enabled ? (
            <div className="skills-actions">
              <Button tone="primary" onClick={() => startThreadWithSkill(selected)}>
                Use in a new thread
              </Button>
            </div>
          ) : null}
        </header>
        {selected.description ? (
          <div className="skills-detail">
            <section>
              <h2>What it does</h2>
              <p>{selected.description}</p>
            </section>
          </div>
        ) : null}
      </section>
    );
  }

  // No workspace, or a library with nothing in it: the header stays, with no count, no filters and no grid.
  if (entries.length === 0) {
    return (
      <section className="skills-page" aria-label="Skills">
        <header className="skills-heading">
          <NarrowBack />
          <h1 ref={heading} tabIndex={-1} data-page-title>
            Skills
          </h1>
        </header>
        <div className="skills-content">
          <div className="skills-empty">
            <Book size={16} aria-hidden />
            <p role="status">No skills in this workspace yet.</p>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="skills-page" aria-label="Skills">
      <header className="skills-heading">
        <NarrowBack />
        <h1 ref={heading} tabIndex={-1} data-page-title>
          Skills
        </h1>
        <span className="skills-subtitle">
          {shown.length === 0 ? `0 of ${entries.length}` : [`${entries.length} built in`, ...crewParts].join(" · ")}
        </span>
      </header>
      <div className="skills-filters" role="group" aria-label="Filter skills">
        {categories.length > 0
          ? (["All", ...categories] as const).map((value) => (
              <button type="button" className="skills-filter" key={value} aria-pressed={category === value} onClick={() => setCategory(value)}>
                {value}
              </button>
            ))
          : null}
        <label className="skills-search">
          <Search size={13} aria-hidden />
          <span className="skills-search-field">
            <span aria-hidden>Search skills</span>
            <input aria-label="Search skills" placeholder="Search skills" value={search} onChange={(event) => setSearch(event.target.value)} />
          </span>
        </label>
      </div>
      <div className="skills-content" ref={content}>
        {showCrew && crew.proposed.length > 0 ? <ProposedGroup items={crew.proposed} /> : null}
        {showCrew && crew.added.length > 0 ? <AddedGroup items={crew.added} /> : null}
        {showCrew && crew.added.length === 0 ? <p className="skills-group-note">{ADDED_EMPTY_LINE}</p> : null}
        {showCrew ? (
          <h5 className="skills-group-head">
            Built in
            <span className="skills-group-count">{entries.length}</span>
          </h5>
        ) : null}
        <div role="status">
          {shown.length === 0 ? (
            <div className="skills-empty">
              <Search size={16} aria-hidden />
              {query ? (
                <>
                  <b>{`No skills match \u201c${search.trim()}\u201d${category === "All" ? "" : ` in ${category}`}`}</b>
                  <p>Try a skill name or a word from what it does.</p>
                  <Button icon={<X size={14} aria-hidden />} onClick={() => setSearch("")}>
                    Clear search
                  </Button>
                </>
              ) : (
                <b>{`No skills in ${category}`}</b>
              )}
            </div>
          ) : null}
        </div>
        <div className="skills-grid" ref={grid}>
          {shown.map((entry) => (
            <button type="button" className="skills-card" key={entry.name} data-skill={entry.name} data-autofocus={entry.name === opened.current || undefined} onClick={() => {
                // Noted at the press, not when the page opens: going back may come before the page has drawn.
                opened.current = entry.name;
                listScrollTop.current = content.current?.scrollTop ?? 0;
                navigate({ name: "skills", skill: entry.name });
              }}>
              <strong>{entry.name}</strong>
              <span className="skills-description">{entry.description}</span>
              <span className="skills-foot">
                <span className="skills-status">Built in</span>
                {entry.category.trim() ? <span className="skills-category">{entry.category.trim()}</span> : null}
              </span>
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

/** The proposals that wait for a decision, above the skills themselves (KC9, the design's Proposed group). */
function ProposedGroup({ items }: { items: ProposedSkill[] }) {
  return (
    <div className="skills-group">
      <h5 className="skills-group-head">
        Proposed
        <span className="skills-group-count">{items.length}</span>
      </h5>
      <div className="skills-grid">
        {items.map(({ proposal, repo }) => {
          const text = skillText(proposal.text);
          const name = text.name ?? "Proposed skill";
          return (
            <div className="skills-card skills-card-static" key={proposal.id} role="region" aria-label={`Proposed skill ${name}`}>
              <strong>{name}</strong>
              {text.description ? <span className="skills-description">{text.description}</span> : null}
              <span className="skills-foot skills-foot-col">
                <span className="skills-status">Proposed</span>
                <span className="skills-source">{`From ${lessonsWord(proposal.lessons.length)} · Personal`}</span>
              </span>
              <div>
                <Button
                  small
                  tone="primary"
                  className="skills-review"
                  onClick={() => navigate({ name: "settings", section: "crew", crewPage: "skill-proposal", repo: repo.id, id: proposal.id })}
                >
                  Review
                </Button>
              </div>
            </div>
          );
        })}
      </div>
      <p className="skills-group-note">{PROPOSED_GROUP_NOTE}</p>
    </div>
  );
}

/** The skills a person approved from Crew proposals: Personal, and above Built in (KC9, Added from lessons). */
function AddedGroup({ items }: { items: AddedSkill[] }) {
  return (
    <div className="skills-group">
      <h5 className="skills-group-head">
        Added from lessons
        <span className="skills-group-count">{items.length}</span>
      </h5>
      <p className="skills-group-note">{ADDED_GROUP_NOTE}</p>
      <div className="skills-grid">
        {items.map((skill) => (
          <div className="skills-card skills-card-static" key={`${skill.repo.id}:${skill.name}`}>
            <strong>{skill.name}</strong>
            {skill.description ? <span className="skills-description">{skill.description}</span> : null}
            <span className="skills-foot skills-foot-col">
              <span className="skills-status">Personal</span>
              <span className="skills-source">
                {`From ${lessonsWord(skill.lessons)} in `}
                <button
                  type="button"
                  onClick={() => navigate({ name: "settings", section: "crew", crewPage: "lessons", repo: skill.repo.id })}
                >
                  {skill.repo.name}
                </button>
              </span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * One skill approved from Crew proposals, on its own page (KC9): what it does, where it came from, and how a worker gets it.
 * The read behind it carries the skill's name, its one line, the repository and how many lessons it came from; the approval
 * time, the lessons' own evidence and the files of skills-local are not in the contract, so those rows are left out.
 */
function PersonalSkill({ skill, heading }: { skill: AddedSkill; heading: RefObject<HTMLHeadingElement | null> }) {
  const summary: SkillSummary = { name: skill.name, category: "", description: skill.description, enabled: true };
  return (
    <section className="skills-page" aria-label="Skill">
      <header className="skills-heading">
        <NarrowBack to={{ name: "skills" }} />
        <h1 ref={heading} tabIndex={-1} data-page-title>
          {skill.name}
        </h1>
        <span className="skills-subtitle">{`Personal · Added from ${lessonsWord(skill.lessons)} · All your projects`}</span>
        <div className="skills-actions">
          <Button tone="primary" onClick={() => startThreadWithSkill(summary)}>
            Use in a new thread
          </Button>
        </div>
      </header>
      <div className="skills-detail">
        <section>
          <h2>What it does</h2>
          <p>{skill.description}</p>
        </section>
        <section>
          <h2>Where it came from</h2>
          <p>Added by you.</p>
          <p>
            {`From ${lessonsWord(skill.lessons)} in `}
            <button
              type="button"
              className="skills-source-link"
              onClick={() => navigate({ name: "settings", section: "crew", crewPage: "lessons", repo: skill.repo.id })}
            >
              {skill.repo.name}
            </button>
            .
          </p>
          <p>
            The lessons stay on the Lessons page, marked Now a skill. While the skill exists they do not go into briefs.{" "}
            {AVAILABLE}
          </p>
        </section>
      </div>
    </section>
  );
}
