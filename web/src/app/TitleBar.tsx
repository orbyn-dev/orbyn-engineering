import { ChevronLeft, ChevronRight, PanelLeft } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api/client";
import { inDesktopApp, onShellChange, readShellStatus } from "../api/shell";
import { SyncTitleButton, useSignedInAccount } from "../features/account";
import { attentionRank, EnvironmentIdentity } from "../features/workspace";
import { useAsync } from "../lib/useAsync";
import { useReadingNav } from "./readingNav";
import { commandShortcut, formatShortcut } from "../product/shell/shortcuts";
import { togglePhoneSidebar, toggleSidebarDrawer } from "../product/shell/sidebarDrawer";
import type { LayoutWidth } from "../lib/useLayout";
import { useUi } from "../product/state/ui";
import { useWaitingThreadIds } from "../product/state/waitingThreads";
import { useWorkspace } from "../product/state/workspace";
import { goBack, goForward, useNavHistory } from "./navHistory";
import { attachBarDrag } from "./barDrag";
import { WindowControls } from "./WindowControls";
import { WindowMenu } from "./WindowMenu";

const INTERACTIVE =
  "a,button,input,select,textarea,summary,label,.window-controls,[role=button],[role=tab],[role=menuitem],[contenteditable=true],[tabindex]:not([tabindex='-1'])";

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
      // The layer takes the pointer over a page heading, so it carries that heading's tooltip (a long title is cut).
      node.title = (!node.contains(under ?? null) && under?.closest("[title]")?.getAttribute("title")) || "";
    };
    document.addEventListener("pointermove", onMove, { passive: true });
    return () => document.removeEventListener("pointermove", onMove);
  }, [layer]);
}

/**
 * Publishes the width the bar's right end takes (the environment pill, the ledger's waiting count, and on Windows
 * the Menu button and the window controls) as `--bar-end-w`, so a page header's own right-hand actions stop short
 * of it instead of sitting under it.
 */
function useBarEndWidth(node: React.RefObject<HTMLSpanElement | null>) {
  useEffect(() => {
    const element = node.current;
    if (!element) return;
    const root = document.documentElement;
    const publish = () => root.style.setProperty("--bar-end-w", `${element.children.length > 0 ? Math.ceil(element.getBoundingClientRect().width) : 0}px`);
    publish();
    const watch = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    watch?.observe(element);
    const mutations = new MutationObserver(publish);
    mutations.observe(element, { childList: true, subtree: true });
    return () => {
      watch?.disconnect();
      mutations.disconnect();
      root.style.removeProperty("--bar-end-w");
    };
  }, [node]);
}

/** The integrated bar for this window's frame: macOS traffic lights, Windows controls, or the native frame. */
export function useWindowFrame(): "lights" | "controls" | null {
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
  if (!attached) return null;
  if (info.data?.platform === "macos") return "lights";
  if (info.data?.platform === "windows") return "controls";
  return null;
}

/** Threads waiting on the person: an approval, a question or a limit. */
function useWaitingCount(): number {
  const threads = useWorkspace((state) => state.threads);
  const waiting = useWaitingThreadIds();
  return useMemo(
    () =>
      Object.values(threads).filter(
        (thread) =>
          !thread.lifecycle?.archived_at &&
          !thread.lifecycle?.settled_at &&
          thread.lifecycle?.relation !== "side" &&
          attentionRank(thread, waiting.has(thread.id)) === 0,
      ).length,
    [threads, waiting],
  );
}

/** The window's top line: the brand, the page it is on and page-level status. */
export function TitleBar({ layout, navShown }: { layout: LayoutWidth; navShown: boolean }) {
  const phone = layout === "phone";
  const small = layout === "small";
  const frame = useWindowFrame();
  const lights = frame === "lights";
  const controls = frame === "controls";
  const toggleSidebar = useUi((state) => state.toggleSidebar);
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const toggleReadingNav = useReadingNav((state) => state.toggle);
  const ledger = useUi((state) => state.workspaceLayout === "ledger");
  const waiting = useWaitingCount();
  const account = useSignedInAccount();
  // The person's keys, so a tooltip shows the chord a command really answers to.
  const keymap = useAsync("keybindings", (signal) => api.keybindings(signal));

  // What the shell puts on screen, so the button and its label never disagree with the sidebar.
  const shown = navShown;
  // Folding the sidebar away must not strand focus on a row that no longer exists; a docked sidebar
  // keeps its rows mounted, so focus can also be left inside one of them.
  const wasShown = useRef(shown);
  useEffect(() => {
    const active = document.activeElement;
    const stranded =
      !active || active === document.body || (active instanceof Element && active.closest(".sidebar") !== null);
    if (wasShown.current && !shown && stranded) {
      document.querySelector<HTMLElement>("[data-sidebar-toggle]")?.focus();
    }
    wasShown.current = shown;
  }, [shown]);

  const trail = useNavHistory();
  const dragLayer = useRef<HTMLDivElement>(null);
  useDragStrip(dragLayer);
  // The integrated bar drags the window where the system bar is gone: macOS traffic lights and Windows controls.
  useEffect(() => (frame && dragLayer.current ? attachBarDrag(dragLayer.current) : undefined), [frame]);
  // Windows draws a 32 px bar (Engineer Designs v287): the root says so, and the bar and page header heights follow it.
  useEffect(() => {
    if (!controls) return undefined;
    const root = document.documentElement;
    root.dataset.windowFrame = "windows";
    return () => { delete root.dataset.windowFrame; };
  }, [controls]);
  const end = useRef<HTMLSpanElement>(null);
  useBarEndWidth(end);
  const sidebarLabel = shown ? "Hide sidebar" : "Show sidebar";
  const shortcut = (command: string, builtIn: string) => formatShortcut(commandShortcut(keymap.data?.rules, command) ?? builtIn);
  const sidebarTitle = `${sidebarLabel} (${shortcut("sidebar.toggle", "mod+b")})`;
  const backTitle = `Back (${shortcut("navigation.back", "mod+[")})`;
  const forwardTitle = `Forward (${shortcut("navigation.forward", "mod+]")})`;

  // One 44px bar in every layout (the v60 frames): it floats over the top of the page, where the page's own header
  // carries the title, chips and actions on the same centre line, with the sidebar shown or not. Only the phone,
  // whose sidebar has the page to itself, gives the bar a row of its own, and it holds the buttons alone.
  const row = phone;
  return (
    <header className="titlebar"
      data-nav={shown && !small ? "shown" : "hidden"}
      data-bar={row ? "row" : "float"}
      data-lights={lights || undefined}
      data-frame={controls ? "windows" : undefined}
    >
      <div ref={dragLayer} className="titlebar-drag" aria-hidden />
      {lights ? <span className="titlebar-lights" aria-hidden /> : null}
      <button
        type="button"
        className="tbi"
        aria-label={sidebarLabel}
        title={sidebarTitle}
        aria-expanded={shown}
        data-sidebar-toggle
        onClick={
          phone
            ? // The phone draws the sidebar as its home page and as a full-screen sheet over any other
              // page: this closes the sheet, leaves the home page where the sidebar already is, or opens
              // the sheet over the page, so the places in it are reachable from anywhere. The sidebar
              // shortcut runs the same function.
              () => togglePhoneSidebar(shown)
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
      <button type="button" className="tbi" aria-label="Back" title={backTitle} aria-disabled={!trail.back} data-dim={!trail.back || undefined} onClick={() => trail.back && goBack()}>
        <ChevronLeft size={16} aria-hidden />
      </button>
      <button type="button" className="tbi" aria-label="Forward" title={forwardTitle} aria-disabled={!trail.forward} data-dim={!trail.forward || undefined} onClick={() => trail.forward && goForward()}>
        <ChevronRight size={16} aria-hidden />
      </button>
      <span ref={end} className="titlebar-end">
        {account && (small || phone) ? <SyncTitleButton view={account.view} reload={account.reload} /> : null}
        <EnvironmentIdentity />
        {ledger && waiting > 0 ? (
          <span className="titlebar-waiting" role="status">
            <span className="status-dot" aria-hidden />
            {waiting} waiting on you
          </span>
        ) : null}
        {controls ? <WindowMenu /> : null}
        {controls ? <WindowControls /> : null}
      </span>
    </header>
  );
}
