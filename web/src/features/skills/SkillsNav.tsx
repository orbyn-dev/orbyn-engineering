import { Book } from "lucide-react";
import { routeHref, type Route } from "../../state/route";
import { useSkills } from "./skillsStore";
import "./skillsNav.css";

/** The service catalog contains the built-in library, including optional skills. With no library there is no count. */
export function SkillsNav({ route }: { route: Route }) {
  const catalog = useSkills();
  const count = catalog.data?.skills.length ?? 0;
  return (
    <div>
      <div className="side-label">Tools</div>
      <a className="nav-row" href={routeHref({ name: "skills" })} aria-current={route.name === "skills" ? "page" : undefined}>
        <Book size={15} aria-hidden />
        <span className="nav-row-label">Skills</span>
        {count > 0 ? <span className="skills-nav-count">{count}</span> : null}
      </a>
    </div>
  );
}
