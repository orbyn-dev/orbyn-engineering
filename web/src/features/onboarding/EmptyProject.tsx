import { Plus } from "lucide-react";
import { useEffect } from "react";
import { redirect, navigate, routeHref } from "../../state/route";
import { useWorkspace, useVisibleProjects } from "../../state/workspace";
import { Button } from "../../ui/controls";
import { Logo } from "../../ui/Logo";
import { ErrorState, Loading } from "../../ui/states";
import "./onboarding.css";

/**
 * A project with no threads (Engineer Designs, "Blank state and splash"): the 48 px mark, a headline and one
 * way in. It decides only once the project and its thread list are known, so a reload or a deep link never
 * shows it for a project that has threads. A project that has threads, or is not trusted yet, is passed on to
 * the new-thread composer for it, replacing this history entry.
 */
export function EmptyProject({ projectId }: { projectId: string }) {
  const projects = useVisibleProjects();
  const loaded = useWorkspace((state) => state.projects);
  const list = useWorkspace((state) => state.threadIds[projectId]);
  const refreshProjects = useWorkspace((state) => state.refreshProjects);
  const refreshThreads = useWorkspace((state) => state.refreshThreads);
  const project = projects?.find((entry) => entry.id === projectId);
  const passOn = project !== undefined && list?.data != null && (list.data.length > 0 || !project.trusted);
  useEffect(() => {
    if (passOn) redirect({ name: "new", projectId });
  }, [passOn, projectId]);

  if (!projects) {
    return loaded.error ? (
      <div className="empty-project">
        <ErrorState title="Couldn't load this project" failure={loaded.error} onRetry={() => void refreshProjects()} />
      </div>
    ) : (
      <Loading label="Loading project" rows={3} />
    );
  }
  if (!project) {
    return (
      <div className="empty-project">
        <h1 tabIndex={-1} data-page-title>
          Project not found
        </h1>
        <p>It may have been removed.</p>
        <a className="back-link" href={routeHref({ name: "home" })}>
          Go to threads
        </a>
      </div>
    );
  }
  if (list?.data == null) {
    return list?.error ? (
      <div className="empty-project">
        <ErrorState title="Couldn't load this project's threads" failure={list.error} onRetry={() => void refreshThreads(projectId)} />
      </div>
    ) : (
      <Loading label="Loading project" rows={3} />
    );
  }
  if (passOn) return null;
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
