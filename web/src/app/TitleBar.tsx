import { ChevronLeft, ChevronRight, PanelLeft } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api/client";
import { inDesktopApp, onShellChange, readShellStatus } from "../api/shell";
import { attentionRank } from "../features/workspace/groups";
import { EnvironmentIdentity } from "../features/workspace/EnvironmentIdentity";
import { useAsync } from "../lib/useAsync";
import { navigate, type Route } from "../state/route";
import { useReadingNav } from "./readingNav";
import { toggleSidebarDrawer, useSidebarDrawer } from "./sidebarDrawer";
import type { LayoutWidth } from "../lib/useLayout";
import { useUi } from "../state/ui";
import { useWorkspace } from "../state/workspace";
import { goBack, goForward, useNavHistory } from "./navHistory";
import { attachBarDrag } from "./barDrag";

const INTERACTIVE =
  "a,button,input,select,textarea,summary,label,[role=button],[role=tab],[role=menuitem],[contenteditable=true],[tabindex]:not([tabindex='-1'])";

/**
 * The whole top strip drags the window, but the bar floats over the page, so its own box
 * would swallow clicks on the page's buttons beneath. The drag layer therefore takes the
 * pointer only while nothing interactive is under it.
 */
function useDragStrip(layer: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const node = layer.current;
      if (!node) return;
      const strip = node.getBoundingClientRect();
      if (event.clientY < strip.top || event.clientY > strip.bottom) {
        node.removeAttribute("data-hot");
        return;
      }
      const under = document.elementsFromPoint?.(event.clientX, event.clientY).find((element) => element !== node);
      if (under?.closest(INTERACTIVE) && !node.contains(under)) node.removeAttribute("data-hot");
      else node.setAttribute("data-hot", "");
    };
    document.addEventListener("pointermove", onMove, { passive: true });
    return () => document.removeEventListener("pointermove", onMove);
  }, [layer]);
}

/** True in the desktop app on macOS, where the window's traffic lights sit over the title bar. */
function useTrafficLights(): boolean {
  const [attached, setAttached] = useState(inDesktopApp());
  const info = useAsync("environment-identity", (signal) => api.info(signal));
  useEffect(() => {
    let live = true;
    const read = () => void readShellStatus().then(() => live && setAttached(inDesktopApp()), () => undefined);
    read();
    const stop = onShellChange(read);
    return () => {
      live = false;
      stop();
    };
  }, []);
  return attached && info.data?.platform === "macos";
}

/** Threads waiting on the person: an approval, a question or a limit. */
function useWaitingCount(): number {
  const threads = useWorkspace((state) => state.threads);
  return useMemo(
    () =>
      Object.values(threads).filter(
        (thread) =>
          !thread.lifecycle?.archived_at &&
          !thread.lifecycle?.settled_at &&
          thread.lifecycle?.relation !== "side" &&
          attentionRank(thread) === 0,
      ).length,
    [threads],
  );
}

/** The window's top line: the brand, the page it is on and page-level status. */
export function TitleBar({ layout }: { route: Route; layout: LayoutWidth }) {
  const phone = layout === "phone";
  const small = layout === "small";
  const drawerOpen = useSidebarDrawer((state) => state.open);
  const lights = useTrafficLights();
  const collapsed = useUi((state) => state.sidebarCollapsed);
  const toggleSidebar = useUi((state) => state.toggleSidebar);
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const readingNav = useReadingNav((state) => state.open);
  const toggleReadingNav = useReadingNav((state) => state.toggle);
  const ledger = useUi((state) => state.workspaceLayout === "ledger");
  const waiting = useWaitingCount();

  const shown = small ? drawerOpen : reading ? readingNav : !collapsed;
  // Folding the sidebar away must not strand focus on a row that no longer exists.
  const wasShown = useRef(shown);
  useEffect(() => {
    if (wasShown.current && !shown && (!document.activeElement || document.activeElement === document.body)) {
      document.querySelector<HTMLElement>("[data-sidebar-toggle]")?.focus();
    }
    wasShown.current = shown;
  }, [shown]);

  const trail = useNavHistory();
  const dragLayer = useRef<HTMLDivElement>(null);
  useDragStrip(dragLayer);
  useEffect(() => (dragLayer.current ? attachBarDrag(dragLayer.current) : undefined), []);
  const sidebarLabel = shown ? "Hide sidebar" : "Show sidebar";

  // One 44px bar in every layout (the v60 frames): it floats over the top of the page, where the page's own header
  // carries the title, chips and actions on the same centre line, with the sidebar shown or not. Only the phone,
  // whose sidebar is its home page, gives the bar a row of its own, and it holds the buttons alone.
  const row = phone;
  return (
    <header className="titlebar"
      data-nav={shown && !small ? "shown" : "hidden"}
      data-bar={row ? "row" : "float"}
      data-lights={lights || undefined}>
      <div ref={dragLayer} className="titlebar-drag" aria-hidden />
      {lights ? <span className="titlebar-lights" aria-hidden /> : null}
      <button
        type="button"
        className="tbi"
        aria-label={sidebarLabel}
        aria-expanded={shown}
        data-sidebar-toggle
        onClick={
          phone
            ? // On the phone the sidebar is the home page: it opens by going home and closes by leaving it.
              () => navigate(shown ? { name: "new", projectId: null } : { name: "home" })
            : small
              ? // On a small window it is a drawer over the thread.
                toggleSidebarDrawer
              : reading
                ? toggleReadingNav
                : toggleSidebar
        }
      >
        <PanelLeft size={16} aria-hidden />
      </button>
      <button type="button" className="tbi" aria-label="Back" aria-disabled={!trail.back} data-dim={!trail.back || undefined} onClick={() => trail.back && goBack()}>
        <ChevronLeft size={16} aria-hidden />
      </button>
      <button type="button" className="tbi" aria-label="Forward" aria-disabled={!trail.forward} data-dim={!trail.forward || undefined} onClick={() => trail.forward && goForward()}>
        <ChevronRight size={16} aria-hidden />
      </button>
      <span className="titlebar-end">
        <EnvironmentIdentity />
                {ledger && waiting > 0 ? (
          <span className="titlebar-waiting" role="status">
            <span className="status-dot" aria-hidden />
            {waiting} waiting on you
          </span>
        ) : null}
      </span>
    </header>
  );
}
