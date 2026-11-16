import { useClearSelectionOnLeave } from "../../lib/clearSelection";
import { CloneJobs, CommandPalette, FoldersNotice, GitHubStarPrompt, QuitConfirmation, Sidebar } from "../../features/workspace";
import { useEffect, useLayoutEffect, useRef, type CSSProperties } from "react";
import { SettingsPage } from "../../features/settings";
import { Toasts } from "../../product/ui/Toasts";
import { usePanelMotionRoot } from "../../product/panels/panelAnimations";
import { useTaskRunNotifications } from "../taskNotifications";
import { useLayoutWidth } from "../../lib/useLayout";
import { navigate, routeHref, useRoute } from "../../product/state/route";
import { DRAFT_LAYOUT, useUi } from "../../product/state/ui";
import { useVisibleProjects, useWorkspace } from "../../product/state/workspace";
import { TitleBar } from "../TitleBar";
import { useReadingNav } from "../readingNav";
import { useSidebarDrawer } from "../../product/shell/sidebarDrawer";
import { useEscapeClose } from "../../ui/useEscapeClose";
import { PreviewFloat } from "../../features/panels";
import { startAppDataRefresh } from "./dataRefresh";
import { HIDDEN_POLL_EVERY, lastFull, loadAll, loadEngines } from "./loading";
import { POLL_MS, quickFocus, quickTick } from "../pollSchedule";
import { useGlobalKeys } from "./keys";
import { BackgroundAnnouncer, useThreadNotifications } from "./notifications";
import { useRouteFocus, useToastClearance } from "./focus";
import { RouteView, type PageRoute } from "./routes";

export function Shell() {
  const route = useRoute();
  const layout = useLayoutWidth();
  const projects = useVisibleProjects();
  const collapsed = useUi((state) => state.sidebarCollapsed);
  const sidebarWidth = useUi((state) => state.sidebarWidth);
  const behindSettings = useRef<PageRoute>({ name: "home" });
  const main = useRef<HTMLElement>(null);
  const routeKey = routeHref(route);
  const threadReady = useWorkspace((state) => route.name !== "thread" || state.threads[route.id] !== undefined);

  const layoutKey =
    route.name === "thread" ? route.id : route.name === "new" || route.name === "home" ? DRAFT_LAYOUT : null;
  useLayoutEffect(() => {
    if (layoutKey) useUi.getState().setLayoutKey(layoutKey);
  }, [layoutKey]);
  const palette = useUi((state) => state.palette);
  const setPalette = useUi((state) => state.setPalette);
  useGlobalKeys(route);
  useThreadNotifications(route.name === "thread" ? route.id : null);
  useTaskRunNotifications();
  usePanelMotionRoot(routeKey);
  useRouteFocus(routeKey, route.name, threadReady, main);
  useClearSelectionOnLeave(routeKey);
  useToastClearance(`${routeKey}:${threadReady}`);

  useEffect(() => {
    void loadAll();
    void loadEngines();
    const stopRefresh = startAppDataRefresh();
    let ticks = 0;
    const timer = setInterval(() => {
      ticks += 1;
      // A hidden window keeps up while work runs, or its alerts would never fire, and
      // looks in now and then so it hears of work started elsewhere.
      const running = Object.values(useWorkspace.getState().threads).some((thread) => thread.active_run);
      if (!document.hidden || running || ticks % HIDDEN_POLL_EVERY === 0) void loadAll(quickTick(ticks));
    }, POLL_MS);
    // Coming back to the window: what changed while it was away, Git state included.
    const onFocus = () => void loadAll(quickFocus(Date.now(), lastFull));
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      stopRefresh();
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  const needsProject = projects !== null && projects.length === 0;
  const overlay = route.name === "settings";
  const underneath = overlay ? behindSettings.current : route;
  if (!overlay) behindSettings.current = route;
  const onHome = underneath.name === "home" && !needsProject;
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const readingNav = useReadingNav((state) => state.open);
  // Small windows (600 to 899 px) fold the sidebar into a drawer over the thread.
  const small = layout === "small";
  const drawerOpen = useSidebarDrawer((state) => state.open);
  const closeDrawer = useSidebarDrawer((state) => state.close);
  const showNav = layout === "phone" ? onHome && route.name !== "settings" : small ? drawerOpen : reading ? readingNav : !collapsed;
  // A docked sidebar stays mounted while collapsed: showing it again must not rebuild every row.
  const docked = !small && layout !== "phone" && !reading;
  const navMounted = showNav || docked;
  // Choosing something closes it, and a window that grows past small leaves nothing open behind.
  useEffect(() => {
    closeDrawer();
  }, [routeKey, small, closeDrawer]);
  useEscapeClose(small && drawerOpen, closeDrawer, () => document.querySelector<HTMLElement>("[data-sidebar-toggle]"));
  const dismissDrawer = () => {
    closeDrawer();
    document.querySelector<HTMLElement>("[data-sidebar-toggle]")?.focus();
  };

  return (
    <div
      className="shell"
      data-nav={showNav && !small ? "shown" : "hidden"}
      data-drawer={small && drawerOpen ? "open" : undefined}
      data-layout={layout}
      data-reading-nav={readingNav ? "open" : undefined}
      data-page={underneath.name}
      style={{ "--sidebar-w": `${sidebarWidth}px` } as CSSProperties}
    >
      <a className="skip-link" href="#main" onClick={(event) => {
        event.preventDefault();
        main.current?.focus();
      }}>
        Skip to content
      </a>
      <TitleBar layout={layout} />
      {small && drawerOpen ? <div className="drawer-scrim" aria-hidden onClick={dismissDrawer} /> : null}
      {navMounted ? (
        small ? (
          // Choosing a thread or page closes the drawer, even one already open.
          <div className="sidebar-drawer" onClick={(event) => (event.target as Element).closest("a[href]") && dismissDrawer()}>
            <Sidebar route={route} />
          </div>
        ) : (
          <Sidebar route={route} />
        )
      ) : null}
      <BackgroundAnnouncer />
      <QuitConfirmation />
      <PreviewFloat threadId={route.name === "thread" ? route.id : null} />
      <Toasts
        lead={
          <>
            <FoldersNotice />
            <GitHubStarPrompt />
          </>
        }
      />
      <CloneJobs />
      {palette ? <CommandPalette start={palette} onClose={() => setPalette(null)} /> : null}
      {layout === "phone" && onHome && !overlay ? (
        <main id="main" ref={main} tabIndex={-1} hidden />
      ) : (
        <main id="main" ref={main} tabIndex={-1} className="main">
          {route.name === "settings" ? <SettingsPage section={route.section} onClose={() => navigate(underneath)} /> : null}
          {/* The page behind settings stays mounted, so a terminal, preview or scroll position survives. */}
          <div className="main-view" hidden={route.name === "settings"}>
            <RouteView route={underneath} needsProject={needsProject} />
          </div>
        </main>
      )}
    </div>
  );
}
