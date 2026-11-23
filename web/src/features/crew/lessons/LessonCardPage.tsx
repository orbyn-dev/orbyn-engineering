import { useEffect, type ReactNode } from "react";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { Frame } from "../shared/Frame";
import { LessonCard } from "./LessonCard";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * One lesson's card on the real route, opened from a row of the list (Settings › Crew › Lessons). Nothing is drawn, and no
 * lesson route is read, while the service's Crew Preview is off or the route names no lesson or repository. The page reads
 * the project list itself, because a Preview hash can open before the app shell loads the workspace; the card's own read
 * then answers 404 not_found from a service without the lessons routes, and the card draws nothing at all.
 */
export function LessonCardPage({ id, repo }: { id?: string; repo?: string }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null)
    return (
      <Card>
        {failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}
      </Card>
    );
  const project = projects.find((entry) => entry.id === repo) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  if (!id) return <Card><Empty title="No lesson to show" /></Card>;
  return <LessonCard key={`${project.id}:${id}`} id={id} repo={project.id} name={project.name} />;
}
