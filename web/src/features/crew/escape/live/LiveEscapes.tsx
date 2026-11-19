import { useEffect, useState, type ReactNode } from "react";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { Frame } from "../../shared/Frame";
import { LiveEscapeLog } from "./LiveEscapeLog";
import { LiveEscapePossible } from "./LiveEscapePossible";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * The escape log screens on the real routes, opened by hash in a Preview build. Nothing is drawn, and no escape route is
 * read, while the service's Crew Preview is off. A Preview hash opens before the app shell loads the workspace, so the
 * log and the possible list read the project list themselves; one project is shown at a time, the first one listed until
 * a person chooses another from the page's project chip. The detail names its escape in the hash (`?id=`), so it needs
 * no project of its own: the escape says which project it belongs to.
 */
export function LiveEscapes({ view }: { view: "log" | "possible" }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  const [chosen, setChosen] = useState<string | null>(null);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null)
    return <Card>{failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}</Card>;
  const project = projects.find((entry) => entry.id === chosen) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  if (view === "log") return <LiveEscapeLog key={project.id} project={project.id} name={project.name} projects={projects} onPick={setChosen} />;
  return <LiveEscapePossible key={project.id} project={project.id} name={project.name} projects={projects} onPick={setChosen} />;
}
