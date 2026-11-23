import { useEffect, type ReactNode } from "react";
import { ChevronLeft } from "lucide-react";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { useAsync } from "../../../../lib/useAsync";
import { Frame } from "../../shared/Frame";
import { crewApi } from "./api";
import { useCrewPreview } from "./useCrew";
import { Chip } from "./parts";
import { WatchChanges } from "./WatchPanel";
import type { WatchFrameId } from "./watchNames";
import { watchChangesView } from "./watchModel";

/**
 * The watch frames that are one section of a page, drawn on the real watch routes over the dev mock: `watch-history`
 * is the change table alone, as its frame draws it. Everything else of KC6 draws on the app's own route
 * (`#/crew/background`), so the list, the create dialog and a watch's page need no frame here. Nothing is drawn while
 * Crew Preview is off, and a service whose watch route answers 404 draws nothing either (the read's own answer is used).
 */

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

interface Read {
  crew: Parameters<typeof watchChangesView>[1];
  changes: Parameters<typeof watchChangesView>[0];
  status: Parameters<typeof watchChangesView>[3];
}

/** The newest watch of the first project, with its changes: the same two reads the Background page makes. */
function WatchHistoryRead({ project }: { project: string }) {
  const read = useAsync<Read | null>(`crew-watch-history:${project}`, async (signal) => {
    const page = await crewApi.backgroundCrews({ project, offset: 0, limit: 100 }, signal);
    const watched = page.crews.find((one) => one.trigger.kind === "watch");
    if (!watched) return null;
    const [status, changes] = await Promise.all([
      crewApi.backgroundWatch(watched.id, signal),
      crewApi.backgroundWatchChanges(watched.id, { offset: 0, limit: 20 }, signal),
    ]);
    return { crew: watched, changes: changes.changes, status };
  });
  if (read.status !== "ready" || !read.data) return null;
  const { crew, changes, status } = read.data;
  const view = watchChangesView(changes, crew, new Date(), status);
  return (
    <div className="ap-page" aria-label={crew.name + " watch changes"}>
      <header className="ap-set-head ap-watch-head">
        <span className="ap-watch-back">
          <ChevronLeft size={14} aria-hidden />
          Background crews
        </span>
        <b>{crew.name}</b>
        <Chip tone="plain">Watch</Chip>
        <Chip tone="plain">Preview</Chip>
      </header>
      <WatchChanges view={view} />
      {view.note ? null : <p className="ap-bg-note">Checks that found the same result as the one before are not listed.</p>}
    </div>
  );
}

function WatchHistory() {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null)
    return <Card>{failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}</Card>;
  const project = projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  return <Card><WatchHistoryRead key={project.id} project={project.id} /></Card>;
}

/** The watch frames drawn in the Preview chunk. Loaded only through the Preview route. */
export const WATCH_SCREENS: Record<WatchFrameId, () => ReactNode> = {
  "watch-history": () => <WatchHistory />,
};
