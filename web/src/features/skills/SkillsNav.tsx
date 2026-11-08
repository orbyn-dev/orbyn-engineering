import { Book } from "lucide-react";
import { api } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { routeHref, type Route } from "../../state/route";
import "./skillsNav.css";

/** The service catalog contains the built-in library, including optional skills. */
export function SkillsNav({ route }: { route: Route }) {
  const catalog = useAsync("skills-nav", (signal) => api.skills(signal));
  return (
    <div>
      <div className="side-label">Tools</div>
      <a className="nav-row" href={routeHref({ name: "skills" })} aria-current={route.name === "skills" ? "page" : undefined}>
        <Book size={15} aria-hidden />
        <span className="nav-row-label">Skills</span>
        {catalog.data ? <span className="skills-nav-count">{catalog.data.skills.length}</span> : null}
      </a>
    </div>
  );
}
