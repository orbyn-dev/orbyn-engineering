import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { arrangeThreads, copyReference, pinThreads, settleThreads, threadsFlat, unpinThreads, unsettleThreads, useDrawnOrder } from "../../features/workspace";
import { toFailure } from "../../lib/useAsync";
import { layoutForWidth } from "../../lib/useLayout";
import { navigate, type Route } from "../../product/state/route";
import { useUi, type ContextTab, type ThemeChoice } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { toggleSidebarDrawer } from "../../product/shell/sidebarDrawer";
import { useReadingNav } from "../readingNav";

const NEXT_THEME: Record<ThemeChoice, ThemeChoice> = { system: "light", light: "dark", dark: "system" };

export const PANEL_KEYS: Record<string, ContextTab> = {
  Digit1: "diff",
  Digit2: "terminal",
  Digit3: "activity",
};

export const editing = (target: EventTarget | null) =>
  target instanceof HTMLElement && target.matches("input, textarea, select, [contenteditable='true']");

export const openThread = (route: Route) => {
  const id = route.name === "thread" ? route.id : null;
  return id ? useWorkspace.getState().threads[id] : undefined;
};

function jumpRows() {
  const { threads } = useWorkspace.getState();
  // The sidebar's own order, when it is on screen: jumps must match the numbers it shows.
  const drawn = useDrawnOrder.getState().ids;
  if (drawn) {
    return drawn
      .map((id) => threads[id])
      .filter((thread): thread is Thread => thread !== undefined && !thread.lifecycle?.settled_at);
  }
  return threadsFlat(arrangeThreads(Object.values(threads), Date.now())).filter(
    (thread) => !thread.lifecycle?.settled_at,
  );
}

function stepThread(route: Route, step: number): boolean {
  const rows = jumpRows();
  if (rows.length === 0) return false;
  const at = rows.findIndex((thread) => route.name === "thread" && thread.id === route.id);
  const next = rows[(at + step + rows.length) % rows.length];
  if (next) navigate({ name: "thread", id: next.id });
  return true;
}

/** Commands the shell owns; the rest belong to whatever registered for them. */
export function runShellCommand(command: string, route: Route): boolean {
  const ui = useUi.getState();
  const thread = openThread(route);
  const jump = /^thread\.jump\.(\d)$/.exec(command);
  if (jump) {
    const row = jumpRows()[Number(jump[1]) - 1];
    if (row) navigate({ name: "thread", id: row.id });
    return row !== undefined;
  }
  switch (command) {
    case "sidebar.toggle":
      if (layoutForWidth(window.innerWidth) === "small") toggleSidebarDrawer();
      else if (ui.workspaceLayout === "reading") useReadingNav.getState().toggle();
      else ui.toggleSidebar();
      return true;
    case "commandPalette.toggle":
      ui.setPalette(ui.palette ? null : "root");
      return true;
    case "usage.open":
      navigate({ name: "usage" });
      return true;
    case "settings.open":
      navigate({ name: "settings", section: null });
      return true;
    case "updates.check":
      navigate({ name: "settings", section: "maintenance" });
      api.checkUpdates().then(
        (status) => useWorkspace.getState().putUpdates(status),
        (failure: unknown) => ui.pushToast(`Couldn't check for updates. ${toFailure(failure).message}`),
      );
      return true;
    case "appearance.cycle":
      ui.setTheme(NEXT_THEME[ui.theme]);
      return true;
    case "theme.select":
    case "themeEditor.toggle":
      navigate(route.name === "settings" ? { name: "home" } : { name: "settings", section: "appearance" });
      return true;
    case "filePicker.toggle":
      if (route.name !== "thread") return false;
      ui.setPalette(ui.palette ? null : "files");
      return true;
    case "projectSearch.toggle":
      if (route.name !== "thread") return false;
      ui.setPalette(ui.palette ? null : "search");
      return true;
    case "navigation.back":
      window.history.back();
      return true;
    case "navigation.forward":
      window.history.forward();
      return true;
    case "chat.new":
    case "chat.newLocal":
      navigate({ name: "new", projectId: route.name === "project" ? route.projectId : (thread?.project_id ?? null) });
      return true;
    case "chat.newWithoutProject":
      navigate({ name: "new", projectId: null });
      return true;
    case "workspace.focusNext":
      return cycleFocus(1);
    case "workspace.focusPrevious":
      return cycleFocus(-1);
    case "thread.previous":
      return stepThread(route, -1);
    case "thread.next":
      return stepThread(route, 1);
    default:
      break;
  }
  if (route.name !== "thread") return false;
  switch (command) {
    case "preview.toggle":
      useUi.getState().togglePanel("browser");
      return true;
    case "terminal.toggle":
      ui.togglePanel("terminal");
      return true;
    case "diff.toggle":
      ui.togglePanel("diff");
      return true;
    case "rightPanel.toggle":
      ui.setPanelOpen(ui.panelOpen !== true);
      return true;
    case "rightPanel.close":
      if (ui.panelOpen !== true) return false;
      ui.closePanel();
      return true;
    case "thread.stop":
      if (!thread?.active_run) return false;
      void api.cancelRun(thread.active_run.id);
      return true;
    case "thread.settle":
      if (!thread) return false;
      void (thread.lifecycle?.settled_at ? unsettleThreads([thread]) : settleThreads([thread]));
      return true;
    case "thread.pin":
      if (!thread) return false;
      void (thread.lifecycle?.pinned_at ? unpinThreads([thread]) : pinThreads([thread]));
      return true;
    case "thread.copyReference":
      if (!thread) return false;
      void copyReference(thread);
      return true;
    default:
      return false;
  }
}

/** The window's main areas, for moving between them without a pointer. */
const FOCUS_AREAS = [
  "nav[aria-label='Workspace'] .thread-row a, nav[aria-label='Workspace'] a",
  ".transcript, [aria-label='Thread messages']",
  ".composer textarea",
  "[aria-label='Thread context'] button, .details button",
];

function cycleFocus(step: number): boolean {
  const areas = FOCUS_AREAS.map((selector) => document.querySelector<HTMLElement>(selector)).filter((area) => area !== null);
  if (areas.length === 0) return false;
  const current = areas.findIndex((area, at) => {
    const selector = FOCUS_AREAS[at] ?? "";
    return area === document.activeElement || document.activeElement?.closest(selector.split(",")[0]?.split(" ")[0] ?? "") != null;
  });
  const next = areas[(current + step + areas.length) % areas.length];
  if (next && !next.hasAttribute("tabindex") && next.tagName === "DIV") next.tabIndex = -1;
  next?.focus();
  return true;
}
