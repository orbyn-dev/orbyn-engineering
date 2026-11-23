import { useAsync } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { Button } from "../../../ui/controls";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useVisibleProjects } from "../../../product/state/workspace";
import { EntryRow } from "../shared/EntryRow";
import { lessonsApi } from "./api";

/**
 * The lessons row of Settings › Crew (KC5), drawn after Escapes. It is drawn only once one probe read of the first
 * visible project's list has answered 200: while that read runs, and when it answers 404 not_found (a service without
 * the lessons routes) or any other failure, nothing is drawn at all. With Crew Preview off, or with no project, no
 * lessons route is read and nothing is drawn. Opening the page reads on its own; the row sends nothing.
 */
export function LessonsEntry() {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const project = projects?.[0];
  if (!on || !project) return null;
  return <LessonsProbe key={project.id} repo={project.id} />;
}

function LessonsProbe({ repo }: { repo: string }) {
  const probe = useAsync(`crew-lessons-probe:${repo}`, (signal) => lessonsApi.list(repo, "all", signal));
  if (probe.status !== "ready") return null;
  return (
    <EntryRow
      id="lessons-entry-title"
      title="Lessons"
      action={
        <Button small aria-label="Open Lessons" onClick={() => navigate({ name: "settings", section: "crew", crewPage: "lessons" })}>
          Open Lessons
        </Button>
      }
    >
      What Crew has learned on this device from a failure that repeated, an escape or a revert you confirmed, and your
      corrections. You can add one by hand, and read, edit, turn off, promote, move back and delete each one. Turning
      lessons off for a repository stops them reaching its briefs.
    </EntryRow>
  );
}
