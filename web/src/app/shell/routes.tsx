import { lazy, Suspense, useEffect } from "react";
import { WithFilePanel } from "../../features/panels";
import { useUi } from "../../product/state/ui";
import { Empty, Loading } from "../../product/ui/states";
import { LinkButton } from "../../ui/controls";
import { routeHref, type Route } from "../../product/state/route";

export type PageRoute = Exclude<Route, { name: "settings" }>;

/**
 * Every route screen is a dynamic import, so a cold load runs the shell and the screen of the open
 * address instead of all of them; a screen's code arrives when its address first opens. Each screen
 * comes through its feature's front door, the only module the app may import from a feature, so one
 * import per feature keeps one chunk (see the groups in vite.config.ts), and two screens of the same
 * feature share the request already made for the first.
 */
const DraftView = lazy(() => import("../../features/onboarding").then((module) => ({ default: module.DraftView })));
const EmptyProject = lazy(() => import("../../features/onboarding").then((module) => ({ default: module.EmptyProject })));
const TrustFolder = lazy(() => import("../../features/onboarding").then((module) => ({ default: module.TrustFolder })));
const WelcomeHost = lazy(() => import("../../features/onboarding").then((module) => ({ default: module.WelcomeHost })));
const WelcomePage = lazy(() => import("../../features/onboarding").then((module) => ({ default: module.WelcomePage })));
const CrewSurface = lazy(() => import("../../features/crew").then((module) => ({ default: module.CrewSurface })));
const IssuesPage = lazy(() => import("../../features/issues").then((module) => ({ default: module.IssuesPage })));
const PullsPage = lazy(() => import("../../features/pulls").then((module) => ({ default: module.PullsPage })));
const ScheduledTasksPage = lazy(() => import("../../features/settings").then((module) => ({ default: module.ScheduledTasksPage })));
const SkillsPage = lazy(() => import("../../features/skills").then((module) => ({ default: module.SkillsPage })));
const ThreadView = lazy(() => import("../../features/thread").then((module) => ({ default: module.ThreadView })));
const UsagePage = lazy(() => import("../../features/usage").then((module) => ({ default: module.UsagePage })));

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

/**
 * Nothing drawn: reports the moment the screen for an address is on the page. Focus follows the
 * screen, so the shell waits for this rather than moving focus to a page that has not arrived.
 */
export function ScreenPainted({ screenKey, onScreen }: { screenKey: string; onScreen: (key: string) => void }) {
  useEffect(() => {
    onScreen(screenKey);
  }, [screenKey, onScreen]);
  return null;
}

export function RouteView({
  route,
  needsProject,
  onScreen,
}: {
  route: PageRoute;
  needsProject: boolean;
  onScreen: (key: string) => void;
}) {
  const page = (
    // The boundary sits inside the page's own frame, so the file panel, the title bar and the
    // sidebar stay put while a screen's chunk loads, and Loading draws nothing at all unless the
    // wait outlasts 400 ms.
    <Suspense fallback={<Loading label="Opening the page" />}>
      <PageScreen route={route} needsProject={needsProject} />
      <ScreenPainted screenKey={routeHref(route)} onScreen={onScreen} />
    </Suspense>
  );
  return route.name === "home" || route.name === "new" || route.name === "project" ? (
    <WithFilePanel>{page}</WithFilePanel>
  ) : (
    page
  );
}

function PageScreen({ route, needsProject }: { route: PageRoute; needsProject: boolean }) {
  switch (route.name) {
    case "home":
      return <Home needsProject={needsProject} />;
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
      return <DraftView key={route.projectId ?? "draft"} projectId={route.projectId} />;
    case "project":
      return <EmptyProject key={route.projectId} projectId={route.projectId} />;
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
