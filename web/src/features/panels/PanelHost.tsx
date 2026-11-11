import { CloudOff, Maximize2, Minimize2, MoreHorizontal, Plus, RefreshCw, X } from "lucide-react";
import type { KeyboardEvent, ReactNode } from "react";
import { flushSync } from "react-dom";
import { captureFocusReturn } from "../../ui/focusReturn";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Thread } from "../../api/types";
import { isFileTab, parseFileTab, useUi, type BuiltinTab, type RightTab } from "../../product/state/ui";
import { Button, IconButton } from "../../ui/controls";
import { ActivityPanel } from "./activity/ActivityPanel";
import { BrowserPanel } from "./browser/BrowserPanel";
import { DiffPanel } from "./diff/DiffPanel";
import { FileTab } from "./files/FileTab";
import { useFileTabNames } from "../workspace";
import { useWorkspace } from "../../product/state/workspace";
import { ToolsTab } from "../tools";
import { api } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { commandShortcut, formatShortcut } from "../../product/shell/shortcuts";
import { PanelEmpty } from "./strip/PanelEmpty";
import { moveIndex, slotShift, useTabDrag } from "./strip/tabDrag";
import { RIGHT_TABS, SLOT_PITCH, orderChanged, tabOf, tabSlots, visibleTabs } from "./strip/tabs";
import { clampPanelWidth, dragPanelWidth, PANEL_DEFAULT, snapPoints } from "./strip/panelWidth";
import { useConnectionKind } from "../../product/state/connection";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";

/** The launcher entries that already have a shortcut, by the command it runs. The rest show none. */
const LAUNCHER_COMMANDS: Partial<Record<BuiltinTab, string>> = { diff: "diff.toggle", browser: "preview.toggle" };
/** The strip's side padding (8 px each side), and the width of the "..." menu. */
const STRIP_PAD = 16;
const MORE_MENU_WIDTH = 190;
/** The one line under "Waiting for the Orbyn service", by tab. The Browser is not here: it stays usable. */
const WAITING_LINE: Partial<Record<BuiltinTab, string>> = {
  diff: "The diff comes back when the service does.",
  activity: "Activity shows again as soon as it is back.",
  side: "Your side chats open again as soon as it is back. Nothing is lost.",
  tools: "The tools list comes back when the service does.",
};
/** Tabs that need this device's network. Diff, Files, Activity and the terminal keep working offline. */
const NEEDS_NETWORK: readonly RightTab[] = ["browser", "side"];

interface PanelDrag {
  panel: HTMLElement;
  handle: HTMLElement;
  pointerId: number;
  right: number;
  layoutKey: string;
  startWidth: string;
  /** The width this drag last wrote to the panel, or null before the first move. */
  writtenWidth: string | null;
  current: number;
}

/**
 * The panel's left edge. Dragging it resizes the panel, snapping to 380, 520 or 720 px within 12 px
 * and staying between 320 px and 60% of the window; a pill shows the width and faint ticks mark the
 * snap points while it moves. Double click returns to 380 px. The width is saved when the drag ends.
 */
export function PanelResizer({ drawer = false }: { drawer?: boolean }) {
  const setWidth = useUi((state) => state.setPanelWidth);
  const layoutKey = useUi((state) => state.layoutKey);
  const [dragging, setDragging] = useState<number | null>(null);
  const drag = useRef<PanelDrag | null>(null);
  // What the panel renders for a thread's width: the same rule as its style, so a restore never differs from a render.
  const widthOf = (panelWidth: number | null) => {
    const width = panelWidth ?? (drawer ? undefined : PANEL_DEFAULT);
    return width === undefined ? "" : `${width}px`;
  };
  const widthOfRef = useRef(widthOf);
  widthOfRef.current = widthOf;

  /** Lets go of the pointer and clears the drag's marks on the panel. */
  const release = (active: PanelDrag) => {
    drag.current = null;
    delete active.panel.dataset.resizing;
    if (active.handle.hasPointerCapture?.(active.pointerId)) active.handle.releasePointerCapture(active.pointerId);
  };

  // A drag cut short by the handle unmounting or the thread changing must not keep selection,
  // capture or a width that belongs to another thread's layout.
  useEffect(() => () => {
    const active = drag.current;
    if (!active) return;
    release(active);
    setDragging(null);
    const current = useUi.getState();
    if (current.layoutKey !== active.layoutKey) {
      active.panel.style.width = widthOfRef.current(current.panelWidth);
    } else if (active.writtenWidth !== null && active.panel.style.width === active.writtenWidth) {
      active.panel.style.width = active.startWidth;
    }
  }, [layoutKey]);

  const stop = (pointerId: number) => {
    const active = drag.current;
    if (!active || active.pointerId !== pointerId) return;
    release(active);
    setDragging(null);
    const current = useUi.getState();
    if (current.layoutKey === active.layoutKey) setWidth(active.current);
    else active.panel.style.width = widthOfRef.current(current.panelWidth);
  };

  return (
    <>
      <div
        className="panel-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize panel"
        title="Drag to resize"
        onDoubleClick={() => setWidth(PANEL_DEFAULT)}
        onPointerDown={(event) => {
          if (event.button !== 0 || drag.current) return;
          const panel = event.currentTarget.parentElement;
          if (!panel) return;
          // Without this the drag selects the text it passes over.
          event.preventDefault();
          event.currentTarget.setPointerCapture(event.pointerId);
          const current = clampPanelWidth(panel.getBoundingClientRect().width, window.innerWidth);
          panel.dataset.resizing = "";
          drag.current = {
            panel,
            handle: event.currentTarget,
            pointerId: event.pointerId,
            right: panel.getBoundingClientRect().right,
            layoutKey,
            startWidth: panel.style.width,
            writtenWidth: null,
            current,
          };
          setDragging(current);
        }}
        onPointerMove={(event) => {
          const active = drag.current;
          if (!active || active.pointerId !== event.pointerId || useUi.getState().layoutKey !== active.layoutKey) return;
          active.current = dragPanelWidth(active.right - event.clientX, window.innerWidth);
          active.writtenWidth = `${active.current}px`;
          active.panel.style.width = active.writtenWidth;
          setDragging(active.current);
        }}
        onPointerUp={(event) => stop(event.pointerId)}
        onPointerCancel={(event) => stop(event.pointerId)}
        onLostPointerCapture={(event) => stop(event.pointerId)}
      />
      {dragging === null ? null : (
        <>
          {snapPoints(window.innerWidth)
            .filter((point) => point !== dragging)
            .map((point) => (
              <span key={point} className="panel-tick" style={{ right: point }} aria-hidden>
                <i>{point}</i>
              </span>
            ))}
          <span className="panel-width" role="status">
            {dragging} px
          </span>
        </>
      )}
    </>
  );
}

/**
 * How the panel sits in the window: "docked" beside the thread, "overlay" over its right side under
 * the header (medium and small windows), or "sheet" in place of the thread (phone).
 */
export type PanelMode = "docked" | "overlay" | "sheet";

const TAB_ID = "context-tab-";
/** A tab's DOM id: a file tab's own id holds a path, which an id (and a selector) cannot. */
const domId = (id: RightTab) => (isFileTab(id) ? `f${[...id].reduce((hash, char) => (hash * 31 + char.charCodeAt(0)) >>> 0, 7).toString(36)}` : id);
const tabSelector = (id: RightTab) => `[data-tab="${CSS.escape(id)}"]`;

export function PanelHost({
  thread,
  mode,
  closing,
  moves,
  maximized,
  sideChat = null,
  onToggleMaximize,
  onReturn,
}: {
  thread: Thread;
  mode: PanelMode;
  closing: boolean;
  /** False while the panel is part of the page as it first appeared. */
  moves: boolean;
  maximized: boolean;
  /** The Side chat tab's content; the thread owns it, so it is handed in. */
  sideChat?: ReactNode;
  onToggleMaximize(): void;
  onReturn(): void;
}) {
  const order = useUi((state) => state.panelTabOrder);
  const closedTabs = useUi((state) => state.panelTabsClosed);
  const rowIds = order.filter((id) => isFileTab(id) || !closedTabs.includes(id));
  // With no surface chosen yet, or the chosen one closed, the panel shows the first in the row; with none open, it is empty.
  const chosen = useUi((state) => state.panelTab);
  const tab: RightTab | null = chosen && rowIds.includes(chosen) ? chosen : (rowIds[0] ?? null);
  const toggle = useUi((state) => state.togglePanel);
  const closeTab = useUi((state) => state.closePanelTab);
  const reopenTab = useUi((state) => state.reopenPanelTab);
  const moveTab = useUi((state) => state.movePanelTab);
  const closeOthers = useUi((state) => state.closeOtherPanelTabs);
  const closeAll = useUi((state) => state.closeAllPanelTabs);
  const resetTabs = useUi((state) => state.resetPanelTabs);
  const width = useUi((state) => state.panelWidth);
  // A header row of its own, with Back to thread: the phone's sheet, and the reading layout's floating drawer.
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const drawer = mode === "sheet" || (reading && mode === "docked");

  const list = useRef<HTMLDivElement>(null);
  const strip = useRef<HTMLElement>(null);
  const kind = useConnectionKind();
  const keymap = useAsync("keybindings", (signal) => api.keybindings(signal));
  // Every tab is a fixed slot. When they don't fit, the last ones fold into "..."; the open tab always shows.
  const [slots, setSlots] = useState(Number.POSITIVE_INFINITY);
  const [promoted, setPromoted] = useState<RightTab | null>(null);
  const [said, setSaid] = useState("");
  const [plusMenu, setPlusMenu] = useState<{ x: number; y: number } | null>(null);
  const [moreMenu, setMoreMenu] = useState<{ x: number; y: number } | null>(null);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number } | null>(null);
  const count = rowIds.length;
  // The line beside the open file's name: the worktree it is in, and for another project's, which project.
  const target = tab ? parseFileTab(tab) : null;
  const names = useFileTabNames(target && target.kind !== "thread" ? target.projectId : "", target && target.kind !== "thread" ? target.worktreeId : "", thread.id);
  // A folder outside Git has nothing to compare with: Changes is offered greyed out, and the empty panel says why.
  // A Jujutsu checkout is no Git repository either, but has changes to show: only a folder with neither is gitless.
  const notGit = useWorkspace((state) => state.projects.data?.find((one) => one.id === thread.project_id)?.is_git === false);
  const jj = useAsync(`jj-gate:${thread.id}:${notGit}`, (signal) => (notGit ? api.jjStatus(thread.id, signal) : Promise.resolve(null)));
  const gitless = notGit && jj.status === "ready" && jj.data === null;
  const subtitle =
    target === null ? null
    : target.kind === "thread" ? (thread.worktree?.branch ?? null)
    : !names.worktree ? null
    : target.projectId === thread.project_id || !names.project ? names.worktree : `${names.project} · ${names.worktree}`;
  const latest = useRef({ count });
  latest.current = { count };
  const refit = useCallback(() => {
    const head = strip.current;
    if (!head) return;
    // A strip with no width yet (not laid out, or hidden) folds nothing.
    if (head.clientWidth === 0) return setSlots(Number.POSITIVE_INFINITY);
    const end = head.querySelector<HTMLElement>(".ctx-end");
    const room = head.clientWidth - STRIP_PAD - (end?.offsetWidth ?? 0);
    setSlots(tabSlots(room, latest.current.count));
  }, []);
  useLayoutEffect(() => {
    const head = strip.current;
    if (!head) return;
    refit();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(refit);
    observer?.observe(head);
    return () => observer?.disconnect();
  }, [refit]);
  useLayoutEffect(refit, [count, refit]);
  // A hidden tab that became the open one by any route keeps the slot it took.
  const { shown, hidden, holder } = visibleTabs(rowIds, tab, promoted, slots);
  useLayoutEffect(() => {
    if (holder !== promoted) setPromoted(holder);
  }, [holder, promoted]);
  const folded = hidden.length > 0;
  const { drag, start: startDrag, wasDrag } = useTabDrag({
    list,
    shown,
    onMove: (id, to) => {
      moveTab(id, moveIndex(order, shown, id, to));
      setSaid("Tab moved");
    },
  });

  const reset: MenuItem = { label: "Reset tab order", icon: <RefreshCw size={14} aria-hidden />, separatorBefore: true, onSelect: resetTabs };
  // The "..." menu: the tabs that do not fit, by name. The one chosen takes the last visible slot.
  const hiddenTabs: MenuItem[] = hidden.map((id) => {
    const { label, icon: Icon } = tabOf(id);
    return {
      label,
      icon: <Icon size={14} aria-hidden />,
      onSelect: () => {
        setPromoted(id);
        toggle(id);
        focusTab(id);
      },
    };
  });
  // The "+" lists everything that can be opened; choosing one already open shows it. Reset tab order shows once the order has changed.
  const launcher: MenuItem[] = [
    ...RIGHT_TABS.map(({ id, label, icon: Icon }) => {
      const key = LAUNCHER_COMMANDS[id] ? commandShortcut(keymap.data?.rules, LAUNCHER_COMMANDS[id]!) : null;
      return { label, icon: <Icon size={14} aria-hidden />, shortcut: key ? formatShortcut(key) : undefined, disabled: gitless && id === "diff", onSelect: () => reopenTab(id) };
    }),
    ...(orderChanged(order) ? [{ ...reset, separatorBefore: true }] : []),
  ];
  const plusButton = useRef<HTMLButtonElement>(null);
  const focusTab = (id: RightTab) =>
    requestAnimationFrame(() => list.current?.querySelector<HTMLElement>(tabSelector(id))?.focus());
  /** Closes a tab, then moves focus by the shared rule: the neighbouring tab, else the "+", never the page. */
  const closeWithFocus = (id: RightTab) => {
    const at = rowIds.indexOf(id);
    const neighbour = rowIds[at + 1] ?? rowIds[at - 1];
    // A neighbour folded into "..." has no slot to hold focus: the tab that opens takes the last slot.
    const hiddenNext = neighbour !== undefined && !shown.includes(neighbour);
    const target =
      (neighbour ? list.current?.querySelector<HTMLElement>(tabSelector(neighbour)) : null) ?? plusButton.current ?? list.current;
    const back = captureFocusReturn(target);
    flushSync(() => closeTab(id));
    back.restore();
    if (hiddenNext) requestAnimationFrame(() => list.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus());
    setSaid("Tab closed");
  };
  // One Tab stop: the chosen tab. Arrows move between tabs, Delete closes one, Alt+Shift+Arrow moves it.
  const onTabKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = tab ? shown.indexOf(tab) : -1;
    const current = (event.target as HTMLElement).closest<HTMLElement>("[data-tab]")?.dataset.tab as RightTab | undefined;
    if (!current) return;
    if (event.altKey && event.shiftKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      const neighbour = shown[shown.indexOf(current) + (event.key === "ArrowLeft" ? -1 : 1)];
      if (!neighbour) return;
      event.preventDefault();
      moveTab(current, order.indexOf(neighbour));
      setSaid("Tab moved");
      focusTab(current);
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && rowIds.length > 1) {
      event.preventDefault();
      closeWithFocus(current);
      return;
    }
    const next =
      event.key === "ArrowRight" ? (at + 1) % shown.length
      : event.key === "ArrowLeft" ? (at - 1 + shown.length) % shown.length
      : event.key === "Home" ? 0
      : event.key === "End" ? shown.length - 1
      : -1;
    const target = shown[next];
    if (!target) return;
    event.preventDefault();
    // The tab already open stays open: toggle would close the panel.
    if (target === tab) return;
    toggle(target);
    focusTab(target);
  };
  // The ⋯ under the strip belongs to the open tab. With one tab, closing it is closing the panel (the ×), so only Maximise shows.
  const maximise: MenuItem[] =
    mode === "sheet" ? [] : [{ label: maximized ? "Restore" : "Maximise panel", icon: maximized ? <Minimize2 size={14} aria-hidden /> : <Maximize2 size={14} aria-hidden />, onSelect: onToggleMaximize }];
  const tabMenuItems: MenuItem[] =
    tab === null
      ? []
      : [
          ...(rowIds.length > 1
            ? [
                { label: "Close tab", icon: <X size={14} aria-hidden />, onSelect: () => closeWithFocus(tab) },
                { label: "Close other tabs", icon: <X size={14} aria-hidden />, onSelect: () => { closeOthers(tab); setSaid("Other tabs closed"); focusTab(tab); } },
                { label: "Close all tabs", icon: <X size={14} aria-hidden />, onSelect: () => { closeAll(); setSaid("All tabs closed. Nothing is open."); plusButton.current?.focus(); } },
              ]
            : []),
          ...maximise.map((item, at) => (at === 0 && rowIds.length > 1 ? { ...item, separatorBefore: true } : item)),
        ];

  return (
    <aside
      className="context"
      aria-label="Thread context"
      data-closing={closing || undefined}
      data-moves={moves || undefined}
      data-maximized={maximized || undefined}
      data-mode={mode}
      style={mode !== "sheet" && !maximized ? { width: width ?? (drawer ? undefined : PANEL_DEFAULT) } : undefined}
    >
      {mode === "sheet" || maximized ? null : <PanelResizer drawer={drawer} />}
      {drawer ? (
        <header className="drawer-head">
          <Button small onClick={onReturn}>
            Back to thread
          </Button>
          <span className="end">
            {mode === "sheet" ? null : (
              <IconButton label={maximized ? "Restore" : "Maximise panel"} onClick={onToggleMaximize}>
                {maximized ? <Minimize2 size={16} aria-hidden /> : <Maximize2 size={16} aria-hidden />}
              </IconButton>
            )}
            <IconButton label="Close side panel" onClick={onReturn}>
              <X size={16} aria-hidden />
            </IconButton>
          </span>
        </header>
      ) : null}
      <header ref={strip} className="ctx-strip">
        <div ref={list} className="ctx-tabs" role={shown.length > 0 ? "tablist" : undefined} aria-label={shown.length > 0 ? "Panel tabs" : undefined} onKeyDown={onTabKey}>
          {shown.map((id) => {
            const { label, icon: Icon } = tabOf(id);
            return (
              <button
                key={id}
                type="button"
                role="tab"
                className="ctx-slot"
                id={`${TAB_ID}${domId(id)}`}
                data-tab={id}
                data-tip={label}
                data-selected={tab === id || undefined}
                data-dragging={drag?.id === id || undefined}
                data-shifted={drag && drag.id !== id && slotShift(shown.indexOf(id), drag.from, drag.to) !== 0 ? "" : undefined}
                style={
                  drag
                    ? drag.id === id
                      ? { transform: `translate(${drag.dx}px, ${drag.dy}px)` }
                      : { transform: `translateX(${slotShift(shown.indexOf(id), drag.from, drag.to)}px)` }
                    : undefined
                }
                aria-label={label}
                aria-selected={tab === id}
                aria-controls={tab === id ? `context-${domId(id)}` : undefined}
                tabIndex={tab === id ? 0 : -1}
                onPointerDown={startDrag(id)}
                onClick={() => {
                  if (!wasDrag() && tab !== id) toggle(id);
                }}
              >
                <Icon size={14} aria-hidden />
              </button>
            );
          })}
          {drag ? <span className="ctx-ins" aria-hidden style={{ left: drag.to * SLOT_PITCH - 1 }} /> : null}
        </div>
        {folded ? (
          <button
            type="button"
            className="ctx-slot"
            data-tip="More tabs"
            aria-label="More tabs"
            aria-haspopup="menu"
            aria-expanded={moreMenu !== null}
            data-open={moreMenu !== null || undefined}
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setMoreMenu(moreMenu ? null : { x: rect.right - MORE_MENU_WIDTH, y: rect.bottom + 4 });
            }}
          >
            <MoreHorizontal size={14} aria-hidden />
          </button>
        ) : null}
        <button
          ref={plusButton}
          type="button"
          className="ctx-slot"
          data-tip="Open a tab"
          aria-label="Open a tab"
          aria-haspopup="menu"
          aria-expanded={plusMenu !== null}
          data-open={plusMenu !== null || undefined}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setPlusMenu(plusMenu ? null : { x: rect.right - 200, y: rect.bottom + 4 });
          }}
        >
          <Plus size={14} aria-hidden />
        </button>
        <div className="ctx-end">
          {drawer ? null : (
            <>
              <IconButton label={maximized ? "Restore" : "Maximise panel"} onClick={onToggleMaximize}>
                {maximized ? <Minimize2 size={14} aria-hidden /> : <Maximize2 size={14} aria-hidden />}
              </IconButton>
              <IconButton label="Close side panel" onClick={onReturn}>
                <X size={14} aria-hidden />
              </IconButton>
            </>
          )}
        </div>
      </header>
      {tab === null ? null : (
        <div className="ctx-title">
          <b>{tabOf(tab).label}</b>
          {subtitle ? <span className="ctx-sub">{subtitle}</span> : null}
          <IconButton
            label="Tab menu"
            pressed={tabMenu !== null}
            aria-haspopup="menu"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setTabMenu(tabMenu ? null : { x: rect.right - 200, y: rect.bottom + 4 });
            }}
          >
            <MoreHorizontal size={14} aria-hidden />
          </IconButton>
        </div>
      )}
      <Presence>
        {plusMenu ? (
          <Menu x={plusMenu.x} y={plusMenu.y} label="Open a tab" items={launcher} onClose={() => setPlusMenu(null)} />
        ) : null}
      </Presence>
      <Presence>
        {moreMenu ? <Menu x={moreMenu.x} y={moreMenu.y} label="More tabs" items={hiddenTabs} onClose={() => setMoreMenu(null)} /> : null}
      </Presence>
      <Presence>
        {tabMenu ? (
          <Menu
            x={tabMenu.x}
            y={tabMenu.y}
            label="Tab menu"
            items={tabMenuItems}
            onClose={() => setTabMenu(null)}
          />
        ) : null}
      </Presence>
      <div className="context-body" data-connection={kind ?? undefined}>
        {kind === "offline" && tab !== null && tab !== "browser" && NEEDS_NETWORK.includes(tab) ? (
          <p className="conn-banner conn-panel-banner" role="status">
            <CloudOff size={13} aria-hidden />
            <span>You’re offline.</span>
          </p>
        ) : null}
        {tab === null ? (
          <PanelEmpty onOpen={reopenTab} gitless={gitless} />
        ) : kind === "service-down" && tab !== "browser" ? (
          <div className="conn-wait">
            <b>Waiting for the Orbyn service</b>
            {isFileTab(tab) ? <span>Files open again as soon as it is back.</span> : WAITING_LINE[tab] ? <span>{WAITING_LINE[tab]}</span> : null}
          </div>
        ) : (
          <div id={`context-${domId(tab)}`} role="tabpanel" aria-labelledby={`${TAB_ID}${domId(tab)}`} className="tabpanel">
            {tab === "browser" ? (
              <BrowserPanel threadId={thread.id} />
            ) : tab === "diff" ? (
              <DiffPanel threadId={thread.id} />
            ) : isFileTab(tab) ? (
              <FileTab key={tab} id={tab} threadId={thread.id} />
            ) : tab === "activity" ? (
              <ActivityPanel key={thread.id} threadId={thread.id} />
            ) : tab === "tools" ? (
              <ToolsTab key={thread.id} threadId={thread.id} engine={thread.engine} />
            ) : (
              sideChat
            )}
          </div>
        )}
      </div>
      <span className="sr-only" role="status" aria-live="polite">
        {said}
      </span>
    </aside>
  );
}
