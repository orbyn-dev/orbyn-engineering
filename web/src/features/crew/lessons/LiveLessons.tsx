import { useEffect, useState, type ReactNode } from "react";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { Frame } from "../shared/Frame";
import { LessonsList } from "./LessonsList";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * The lessons list on the real route, opened from Settings › Crew. Nothing is drawn, and no lessons route is read,
 * while the service's Crew Preview is off. The page reads the project list itself (a Preview hash can open before the
 * app shell loads the workspace); one project is shown at a time, the first one listed until a person chooses another
 * from the page's project chip. `initial` is the project the address names, falling back to the first one listed when
 * it is not visible.
 */
export function LiveLessons({ view = "list", initial }: { view?: "list"; initial?: string }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  const [chosen, setChosen] = useState<string | null>(initial ?? null);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on || view !== "list") return null;
  if (projects === null)
    return <Card>{failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}</Card>;
  const project = projects.find((entry) => entry.id === chosen) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  return <LessonsList key={project.id} project={project.id} name={project.name} projects={projects} onPick={setChosen} />;
}
