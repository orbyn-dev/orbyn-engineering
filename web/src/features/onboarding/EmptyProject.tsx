import { Plus } from "lucide-react";
import { navigate } from "../../state/route";
import { useWorkspace, useVisibleProjects } from "../../state/workspace";
import { Button } from "../../ui/controls";
import { Logo } from "../../ui/Logo";
import { DraftView } from "./DraftView";
import "./onboarding.css";

/**
 * A project with no threads (Engineer Designs, "Blank state and splash"): the 48 px mark, a headline and one
 * way in. The screen is data-gated: once the project has a thread, or it is not trusted yet, the route opens the
 * new-thread composer for it instead, as the project's plus button does.
 */
export function EmptyProject({ projectId }: { projectId: string }) {
  const projects = useVisibleProjects();
  const hasThreads = useWorkspace((state) => Object.values(state.threads).some((thread) => thread.project_id === projectId));
  const project = projects?.find((entry) => entry.id === projectId);
  if (!project) return null;
  if (hasThreads || !project.trusted) return <DraftView key={projectId} projectId={projectId} />;
  return (
    <div className="empty-project">
      <span className="empty-project-mark">
        <Logo size={48} />
      </span>
      <h1 tabIndex={-1} data-page-title>
        No threads yet
      </h1>
      <p>Start one in {project.name}.</p>
      <Button tone="primary" small icon={<Plus size={13} aria-hidden />} onClick={() => navigate({ name: "new", projectId })}>
        New thread
      </Button>
    </div>
  );
}
