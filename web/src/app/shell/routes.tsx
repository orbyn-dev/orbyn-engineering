import { DraftView, EmptyProject, TrustFolder, WelcomeHost, WelcomePage } from "../../features/onboarding";
import { CrewSurface } from "../../features/crew";
import { WithFilePanel } from "../../features/panels";
import { IssuesPage } from "../../features/issues";
import { PullsPage } from "../../features/pulls";
import { ScheduledTasksPage } from "../../features/settings";
import { SkillsPage } from "../../features/skills";
import { ThreadView } from "../../features/thread";
import { UsagePage } from "../../features/usage";
import { useUi } from "../../product/state/ui";
import { Empty } from "../../product/ui/states";
import { LinkButton } from "../../ui/controls";
import { routeHref, type Route } from "../../product/state/route";

export type PageRoute = Exclude<Route, { name: "settings" }>;

function Home({ needsProject }: { needsProject: boolean }) {
  // The welcome steps take the whole page while they are open; with no project, the Welcome page is what they leave.
  const welcome = useUi((state) => state.welcomeStep !== null);
  return (
    <div className="home-stack">
      <WelcomeHost needsProject={needsProject} />
      {welcome ? null : needsProject ? <WelcomePage /> : <DraftView key="draft" projectId={null} />}
    </div>
  );
}

export function RouteView({ route, needsProject }: { route: PageRoute; needsProject: boolean }) {
  switch (route.name) {
    case "home":
      return (
        <WithFilePanel>
          <Home needsProject={needsProject} />
        </WithFilePanel>
      );
    case "pulls":
      return <PullsPage />;
    case "issues":
      return <IssuesPage />;
    case "usage":
      return <UsagePage />;
    case "welcome":
      return <WelcomePage />;
    case "skills":
      return <SkillsPage skill={route.skill} />;
    case "tasks":
      return <ScheduledTasksPage />;
    case "crew":
      return <CrewSurface key={`${route.tab}:${route.task ?? ""}`} tab={route.tab} task={route.task} />;
    case "thread":
      return <ThreadView key={route.id} id={route.id} />;
    case "new":
      return (
        <WithFilePanel>
          <DraftView key={route.projectId ?? "draft"} projectId={route.projectId} />
        </WithFilePanel>
      );
    case "project":
      return (
        <WithFilePanel>
          <EmptyProject key={route.projectId} projectId={route.projectId} />
        </WithFilePanel>
      );
    case "trust":
      return <TrustFolder projectId={route.projectId} threadId={route.threadId} />;
    case "not-found":
      // An address that names no screen says so, in the same empty state every page already uses for a
      // subject that is not there (Engineer Designs, "Loading, empty and error": a plain empty state with
      // one action). No not-found frame exists in the designs, so nothing new is drawn here.
      return (
        <div className="page">
          <Empty title="Page not found">
            <LinkButton href={routeHref({ name: "home" })}>Back to workspace</LinkButton>
          </Empty>
        </div>
      );
  }
}
