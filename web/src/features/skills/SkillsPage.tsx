import { Book, Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { SkillSummary } from "../../generated/SkillSummary";
import { useWaited } from "../../lib/motion";
import { DRAFT_KEY } from "../../state/draftStore";
import { navigate, routeHref } from "../../state/route";
import { useUi } from "../../state/ui";
import { Button } from "../../ui/controls";
import { ErrorState } from "../../ui/states";
import { useSkills } from "./skillsStore";
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
  const slow = useWaited(result.loading && !result.data);
  const [category, setCategory] = useState<string>("All");
  const [search, setSearch] = useState("");
  const heading = useRef<HTMLHeadingElement>(null);
  /** The card that was opened, so going back lands on it. */
  const opened = useRef<string | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!result.data) return;
    if (skill !== undefined && !result.data.skills.some((entry) => entry.name === skill)) {
      // A stale bookmark must not remain behind the catalog in the Back trail.
      window.history.replaceState(window.history.state, "", routeHref({ name: "skills" }));
      window.dispatchEvent(new HashChangeEvent("hashchange"));
      return;
    }
    if (skill !== undefined) {
      opened.current = skill;
      heading.current?.focus();
      return;
    }
    const card = opened.current ? grid.current?.querySelector<HTMLElement>(`[data-skill="${CSS.escape(opened.current)}"]`) : null;
    (card ?? heading.current)?.focus();
    opened.current = null;
  }, [skill, result.data]);

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
  if (!result.data) return null;
  const entries = result.data.skills;
  const selected = skill === undefined ? undefined : entries.find((entry) => entry.name === skill);
  const query = search.trim().toLowerCase();
  const shown = entries.filter(
    (entry) => (category === "All" || entry.category.trim() === category) && (!query || `${entry.name} ${entry.description}`.toLowerCase().includes(query)),
  );
  const categories = categoriesOf(entries);

  if (skill !== undefined) {
    if (!selected) return null;
    const label = selected.category.trim();
    return (
      <section className="skills-page" aria-label="Skill">
        <header className="skills-heading">
          <h1 ref={heading} tabIndex={-1}>
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
          <h1 ref={heading} tabIndex={-1}>
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
        <h1 ref={heading} tabIndex={-1}>
          Skills
        </h1>
        <span className="skills-subtitle">{shown.length === 0 ? `0 of ${entries.length}` : `${entries.length} built in`}</span>
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
      <div className="skills-content">
        <div role="status">
          {shown.length === 0 ? (
            <div className="skills-empty">
              <Search size={16} aria-hidden />
              <b>{query ? `No skills match “${search.trim()}”` : "No skills in this category"}</b>
              <p>Try a skill name or a word from what it does.</p>
              <Button
                icon={<X size={14} aria-hidden />}
                onClick={() => {
                  setSearch("");
                  setCategory("All");
                }}
              >
                Clear search
              </Button>
            </div>
          ) : null}
        </div>
        <div className="skills-grid" ref={grid}>
          {shown.map((entry) => (
            <button type="button" className="skills-card" key={entry.name} data-skill={entry.name} onClick={() => navigate({ name: "skills", skill: entry.name })}>
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
