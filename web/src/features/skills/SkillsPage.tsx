import { Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../../api/client";
import type { SkillSummary } from "../../generated/SkillSummary";
import { useWaited } from "../../lib/motion";
import { useAsync } from "../../lib/useAsync";
import { DRAFT_KEY } from "../../state/draftStore";
import { navigate, routeHref } from "../../state/route";
import { useUi } from "../../state/ui";
import { Button } from "../../ui/controls";
import { ErrorState } from "../../ui/states";
import "./skills.css";

type Category = "Engineering" | "Design" | "Creative" | "Security";
const categories: Category[] = ["Engineering", "Design", "Creative", "Security"];

// Catalog categories are more specific than the design's groups. Unrecognized
// categories stay visible in All until the service defines their grouping.
const catalogGroups: Record<string, Category> = {
  "engineering practice": "Engineering",
  "fullstack contracts and delivery": "Engineering",
  "workflow and planning": "Engineering",
  "artifact investigation and rea maintenance": "Engineering",
  "product design": "Design",
  "social content and visuals": "Creative",
  engineering: "Engineering",
  design: "Design",
  creative: "Creative",
  security: "Security",
};
const categoryOf = (skill: SkillSummary): Category | undefined => catalogGroups[skill.category.trim().toLowerCase()];

function startThreadWithSkill(skill: SkillSummary) {
  const { drafts, setDraft, draftProject } = useUi.getState();
  const draft = drafts[DRAFT_KEY] ?? "";
  const separator = draft && !/\s$/.test(draft) ? " " : "";
  setDraft(DRAFT_KEY, `${draft}${separator}$${skill.name} `);
  navigate({ name: "new", projectId: draftProject });
}

export function SkillsPage({ skill }: { skill?: string }) {
  const result = useAsync("skills-page", (signal) => api.skills(signal));
  const slow = useWaited(result.status === "loading" || (result.refreshing && !result.data));
  const [category, setCategory] = useState<Category | "All">("All");
  const [search, setSearch] = useState("");
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!result.data) return;
    if (skill !== undefined && !result.data.skills.some((entry) => entry.name === skill)) {
      // A stale bookmark must not remain behind the catalog in the Back trail.
      window.history.replaceState(window.history.state, "", routeHref({ name: "skills" }));
      window.dispatchEvent(new HashChangeEvent("hashchange"));
      return;
    }
    heading.current?.focus();
  }, [skill, result.data]);

  if (!result.data && result.status !== "error") {
    return slow ? <div className="skills-loading" role="status" aria-label="Loading skills">{Array.from({ length: 6 }, (_, index) => <div className="skills-placeholder" key={index} />)}</div> : null;
  }
  if (!result.data && result.status === "error") {
    return <ErrorState title="Couldn't load skills" failure={result.error} onRetry={() => void result.reload()} />;
  }
  if (!result.data) return null;
  const entries = result.data.skills;
  const selected = skill === undefined ? undefined : entries.find((entry) => entry.name === skill);
  const query = search.trim().toLowerCase();
  const shown = entries.filter((entry) => (category === "All" || categoryOf(entry) === category)
    && (!query || `${entry.name} ${entry.description}`.toLowerCase().includes(query)));

  if (skill !== undefined) {
    if (!selected) return null;
    return (
      <section className="skills-page" aria-label="Skill">
        <header className="skills-heading">
          <h1 ref={heading} tabIndex={-1}>{selected.name}</h1>
          <span className="skills-subtitle">Built in</span>
          {selected.enabled ? <div className="skills-actions"><Button tone="primary" onClick={() => startThreadWithSkill(selected)}>Use in a new thread</Button></div> : null}
        </header>
        {selected?.description ? <div className="skills-detail"><section><h2>What it does</h2><p>{selected.description}</p></section></div> : null}
      </section>
    );
  }

  return (
    <section className="skills-page" aria-label="Skills">
      <header className="skills-heading"><h1 ref={heading} tabIndex={-1}>Skills</h1><span className="skills-subtitle">{entries.length} built in</span></header>
      <div className="skills-filters" aria-label="Filter skills">
        {(["All", ...categories.filter((value) => entries.some((entry) => categoryOf(entry) === value))] as const).map((value) => (
          <button type="button" className="skills-filter" key={value} aria-pressed={category === value} onClick={() => setCategory(value)}>{value}</button>
        ))}
        <label className="skills-search"><Search size={13} aria-hidden /><span className="skills-search-field"><span aria-hidden>Search skills</span><input aria-label="Search skills" placeholder="Search skills" value={search} onChange={(event) => setSearch(event.target.value)} /></span></label>
      </div>
      <div className="skills-content">{shown.length === 0 ? <p className="skills-empty" role="status">No skills match your search.</p> : null}<div className="skills-grid">
        {shown.map((entry) => <button type="button" className="skills-card" key={entry.name} onClick={() => navigate({ name: "skills", skill: entry.name })}>
          <strong>{entry.name}</strong><span className="skills-description">{entry.description}</span><span className="skills-status">Built in</span>
        </button>)}
      </div></div>
    </section>
  );
}
