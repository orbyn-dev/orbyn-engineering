import { useEffect, useState } from "react";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { LiveCoverage } from "./LiveCoverage";
import { LiveSettings } from "./LiveSettings";

/**
 * The Review capacity and coverage screens on the real routes, opened by hash in a Preview build. Nothing is drawn, and no
 * capacity route is read, while the service's Crew Preview is off. A Preview hash opens before the app shell loads the
 * workspace, so this screen reads the project list itself. One project is shown at a time: the first one listed until a person
 * chooses another from the coverage page's project chip. Choosing reads that project's coverage and sends nothing else.
 */
export function LiveCapacity({ view }: { view: "settings" | "coverage" }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  const [chosen, setChosen] = useState<string | null>(null);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null) return failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />;
  const project = projects.find((entry) => entry.id === chosen) ?? projects[0];
  if (!project) return <Empty title="No project to show" />;
  return view === "settings" ? (
    <LiveSettings key={project.id} project={project.id} />
  ) : (
    <LiveCoverage key={project.id} project={project.id} name={project.name} projects={projects} onPick={setChosen} />
  );
}
