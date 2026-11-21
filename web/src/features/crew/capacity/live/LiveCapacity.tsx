import { useEffect, useState, type ReactNode } from "react";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { Frame } from "../../shared/Frame";
import type { CrewOrigin } from "../../../../product/state/route";
import { LiveCoverage } from "./LiveCoverage";
import { LiveSettings } from "./LiveSettings";

/** The page card these screens draw in, with the frame's own padding, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * The Review capacity and coverage screens on the real routes, opened by hash in a Preview build. Nothing is drawn, and no
 * capacity route is read, while the service's Crew Preview is off. A Preview hash opens before the app shell loads the
 * workspace, so this screen reads the project list itself. `initial` is a project the page was opened for (the Coverage link passes the one it came from); `from` is the page that link was on, for the way back; it falls back to the first one listed if that project is not visible. One project is shown at a time: the first one listed until a person
 * chooses another from the coverage page's project chip. Choosing reads that project's coverage and sends nothing else.
 */
export function LiveCapacity({ view, initial, from }: { view: "settings" | "coverage"; initial?: string; from?: CrewOrigin }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  const [chosen, setChosen] = useState<string | null>(initial ?? null);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null) return <Card>{failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}</Card>;
  const project = projects.find((entry) => entry.id === chosen) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  return view === "settings" ? (
    <LiveSettings key={project.id} project={project.id} />
  ) : (
    <LiveCoverage key={project.id} project={project.id} name={project.name} projects={projects} onPick={setChosen} from={from} />
  );
}
