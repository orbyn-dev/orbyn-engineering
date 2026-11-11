import { DraftView, EmptyProject, NoProjects, TrustFolder, WelcomeHost } from "../../features/onboarding";
import { CrewSurface } from "../../features/crew";
import { PullsPage } from "../../features/pulls";
import { ScheduledTasksPage } from "../../features/settings";
import { SkillsPage } from "../../features/skills";
import { ThreadView } from "../../features/thread";
import { UsagePage } from "../../features/usage";
import { useUi } from "../../product/state/ui";
import type { Route } from "../../product/state/route";

export type PageRoute = Exclude<Route, { name: "settings" }>;

function Home({ needsProject }: { needsProject: boolean }) {
  // The welcome steps take the whole page while they are open; the empty home is what they leave.
  const welcome = useUi((state) => state.welcomeStep !== null);
  return (
    <div className="home-stack">
      <WelcomeHost needsProject={needsProject} />
      {welcome ? null : needsProject ? <NoProjects /> : <DraftView key="draft" projectId={null} />}
    </div>
  );
}

export function RouteView({ route, needsProject }: { route: PageRoute; needsProject: boolean }) {
  switch (route.name) {
    case "home":
      return <Home needsProject={needsProject} />;
    case "pulls":
      return <PullsPage />;
    case "usage":
      return <UsagePage />;
    case "skills":
      return <SkillsPage skill={route.skill} />;
    case "tasks":
      return <ScheduledTasksPage />;
    case "crew":
      return <CrewSurface tab={route.tab} />;
    case "thread":
      return <ThreadView key={route.id} id={route.id} />;
    case "new":
      return <DraftView key={route.projectId ?? "draft"} projectId={route.projectId} />;
    case "project":
      return <EmptyProject key={route.projectId} projectId={route.projectId} />;
    case "trust":
      return <TrustFolder projectId={route.projectId} threadId={route.threadId} />;
  }
}
