import { useEffect, useState, type ReactNode } from "react";
import type { Project } from "../../../../api/types";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { Frame } from "../../shared/Frame";
import { LiveEstimate } from "./LiveEstimate";
import { LiveReport } from "./LiveReport";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * The reviewer-backtest screens on the real 4n routes, opened by hash in a Preview build. Nothing is drawn, and no
 * backtest route is read, while the service's Crew Preview is off. A Preview hash opens before the app shell loads the
 * workspace, so the screens read the project list themselves; one project is shown at a time, the first one listed until
 * a person chooses another from the page's project chip. A report frame names its run in the hash ('?id='), so it needs no
 * project of its own: the run says which project it belongs to.
 */
export function LiveBacktest({ view, id }: { view: "estimate" | "report"; id?: string }) {
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
  const project: Project | undefined = projects.find((entry) => entry.id === chosen) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  if (view === "estimate") return <LiveEstimate key={project.id} project={project} projects={projects} onPick={setChosen} />;
  return <LiveReport id={id ?? ""} projects={projects} onPick={setChosen} />;
}
