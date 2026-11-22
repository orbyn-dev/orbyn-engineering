import { useClearSelectionOnLeave } from "../../lib/clearSelection";
import { CloneJobs, CommandPalette, FoldersNotice, GitHubStarPrompt, QuitConfirmation, Sidebar } from "../../features/workspace";
import { ChevronLeft } from "lucide-react";
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
import { tabbables } from "../../ui/focusReturn";
import { Button } from "../../ui/controls";
import { useEscapeClose } from "../../ui/useEscapeClose";
import { PreviewFloat, useTerminalAttention } from "../../features/panels";
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
  useTerminalAttention(route.name === "thread" ? route.id : null);
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
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const readingNav = useReadingNav((state) => state.open);
  // The sidebar is never docked under 900 px. A small window folds it into a drawer over the thread;
  // the phone (under 600 px) draws it as its home page, and as a full-screen sheet over any page that
  // is not home (Engineer Designs, "Phone layout (under 600 px)": the sidebar is a full-screen sheet,
  // never docked or overlaid beside the page).
  const phone = layout === "phone";
  const small = layout === "small";
  const drawerOpen = useSidebarDrawer((state) => state.open);
  const closeDrawer = useSidebarDrawer((state) => state.close);
  const home = underneath.name === "home" && !needsProject && !overlay;
  const showNav = phone ? home || drawerOpen : small ? drawerOpen : reading ? readingNav : !collapsed;
  // A docked sidebar stays mounted while collapsed: showing it again must not rebuild every row.
  const docked = !small && !phone && !reading;
  const navMounted = showNav || docked;
  // The overlay form of the sidebar: the drawer on a small window, the sheet on a page that is not
  // the phone's home. Choosing something closes it, the Esc key closes it, and a window that changes
  // layout leaves nothing open behind.
  const overlaid = drawerOpen && (small || phone);
  useEffect(() => {
    closeDrawer();
  }, [routeKey, small, phone, closeDrawer]);
  useEscapeClose(overlaid, closeDrawer, () => document.querySelector<HTMLElement>("[data-sidebar-toggle]"));
  const dismissDrawer = () => {
    closeDrawer();
    document.querySelector<HTMLElement>("[data-sidebar-toggle]")?.focus();
  };
  // The phone's sheet takes the page's place, so keyboard focus moves into it; Escape and the Close
  // link give it back to the title bar button, which never leaves the screen.
  const sheetNav = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (phone && drawerOpen && sheetNav.current) tabbables(sheetNav.current)[0]?.focus();
  }, [phone, drawerOpen]);
  // A link in the sidebar is a choice: it closes the drawer or the sheet as well as changing the page.
  const chooseInNav = (event: React.MouseEvent) => {
    if ((event.target as Element).closest("a[href]")) dismissDrawer();
  };

  return (
    <div
      className="shell"
      data-nav={showNav && !small && !phone ? "shown" : "hidden"}
      data-drawer={overlaid ? "open" : undefined}
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
      <TitleBar layout={layout} navShown={showNav} />
      {small && drawerOpen ? <div className="drawer-scrim" aria-hidden onClick={dismissDrawer} /> : null}
      {navMounted ? (
        small ? (
          // Choosing a thread or page closes the drawer, even one already open.
          <div className="sidebar-drawer" onClick={chooseInNav}>
            <Sidebar route={route} />
          </div>
        ) : phone && drawerOpen ? (
          <div ref={sheetNav} className="sidebar-sheet" onClick={chooseInNav}>
            <div className="sidebar-sheet-head">
              <Button small tone="quiet" icon={<ChevronLeft size={14} aria-hidden />} onClick={dismissDrawer}>
                Close
              </Button>
            </div>
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
      {phone && home ? (
        // The sidebar is this page, so nothing is drawn under it.
        <main id="main" ref={main} tabIndex={-1} hidden />
      ) : (
        // The page under the phone's sheet stays mounted, so its scroll and its terminal survive.
        <main id="main" ref={main} tabIndex={-1} className="main" hidden={phone && drawerOpen}>
          {route.name === "settings" ? <SettingsPage section={route.section} sub={route.sub} unknown={route.unknown} row={route.row} onClose={() => navigate(underneath)} /> : null}
          {/* The page behind settings stays mounted, so a terminal, preview or scroll position survives. It leaves the
              accessibility tree and the tab order while Settings is open, so the open page owns the only h1. */}
          <div className="main-view" hidden={route.name === "settings"} inert={route.name === "settings" || undefined}>
            <RouteView route={underneath} needsProject={needsProject} />
          </div>
        </main>
      )}
    </div>
  );
}
