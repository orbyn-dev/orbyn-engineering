import { useEffect, type ReactNode } from "react";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { Frame } from "../shared/Frame";
import { Dashboard } from "./Dashboard";

const Card = ({ children }: { children: ReactNode }) => <Frame sheet live>{children}</Frame>;

/**
 * Settings › Crew › Dashboard on the real reads. Nothing is drawn, and no Crew route is read, while the service's Crew Preview is off. The project list is
 * read here when the shell has not loaded it. `wanted` is the project the address named; it falls back to the one a person chose last, then the first listed.
 * Every control here is a read: choosing a project or a period reads again and sends nothing else.
 */
export function LiveDashboard({ wanted }: { wanted?: string }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null) return <Card>{failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}</Card>;
  const repos = projects.filter((entry) => entry.is_git);
  if (repos.length === 0) return <Card><Empty title="No project to show" /></Card>;
  return (
    <Card>
      <Dashboard key={repos.map((entry) => entry.id).join(",")} projects={repos} wanted={wanted} />
    </Card>
  );
}
