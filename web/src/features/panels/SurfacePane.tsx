import { CloudOff, Maximize2, Minimize2, MoreHorizontal, Plus, RefreshCw, X } from "lucide-react";
import type { DragEvent, KeyboardEvent, ReactNode } from "react";
import { flushSync } from "react-dom";
import { captureFocusReturn } from "../../ui/focusReturn";
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { Thread } from "../../api/types";
import { useUi, type RightTab } from "../../state/ui";
import { Button, IconButton } from "../../ui/controls";
import { ActivityPanel } from "./ActivityPanel";
import { BrowserPanel } from "./BrowserPanel";
import { DiffPanel } from "./DiffPanel";
import { FilesPanel } from "./FilesPanel";
import { SideChatPane } from "./SideChatPane";
import { fitTabs, tabOf, tabsChanged } from "./tabs";
import { clampPanelWidth, dragPanelWidth, PANEL_DEFAULT, snapPoints } from "./panelWidth";
import { useConnectionKind } from "../../state/connection";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";

/** The More button: a 24px icon button. */
const MORE_WIDTH = 24;
/** The header's side padding, and the room the "+" takes with its margins. */
const HEAD_PAD = 12;
const PLUS_SPACE = 34;
const MORE_MENU_WIDTH = 210;
/** The one line under "Waiting for the Orbyn service", by tab. The Browser is not here: it stays usable. */
const WAITING_LINE: Partial<Record<RightTab, string>> = {
  diff: "The diff comes back when the service does.",
  files: "Files open again as soon as it is back.",
  activity: "Activity shows again as soon as it is back.",
  side: "Your side chats open again as soon as it is back. Nothing is lost.",
};
/** Tabs that need this device's network. Diff, Files, Activity and the terminal keep working offline. */
const NEEDS_NETWORK: readonly RightTab[] = ["browser", "side"];

/**
 * The panel's left edge. Dragging it resizes the panel, snapping to 380, 520 or 720 px within 12 px
 * and staying between 320 px and 60% of the window; a pill shows the width and faint ticks mark the
 * snap points while it moves. Double click returns to 380 px. The width is saved when the drag ends.
 */
export function PaneResizer() {
  const setWidth = useUi((state) => state.setPanelWidth);
  const [dragging, setDragging] = useState<number | null>(null);
  return (
    <>
      <div
        className="pane-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize panel"
        title="Drag to resize"
        onDoubleClick={() => setWidth(PANEL_DEFAULT)}
        onPointerDown={(event) => {
          const pane = event.currentTarget.parentElement;
          if (!pane) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          const right = pane.getBoundingClientRect().right;
          const handle = event.currentTarget;
          pane.dataset.resizing = "";
          let current = clampPanelWidth(pane.getBoundingClientRect().width, window.innerWidth);
          setDragging(current);
          const move = (moved: PointerEvent) => {
            current = dragPanelWidth(right - moved.clientX, window.innerWidth);
            pane.style.width = `${current}px`;
            setDragging(current);
          };
          const stop = () => {
            handle.removeEventListener("pointermove", move);
            handle.removeEventListener("lostpointercapture", stop);
            delete pane.dataset.resizing;
            setDragging(null);
            setWidth(current);
          };
          handle.addEventListener("pointermove", move);
          handle.addEventListener("lostpointercapture", stop);
        }}
      />
      {dragging === null ? null : (
        <>
          {snapPoints(window.innerWidth)
            .filter((point) => point !== dragging)
            .map((point) => (
              <span key={point} className="pane-tick" style={{ right: point }} aria-hidden>
                <i>{point}</i>
              </span>
            ))}
          <span className="pane-width" role="status">
            {dragging} px
          </span>
        </>
      )}
    </>
  );
}

/**
 * How the pane sits in the window: "docked" beside the thread, "overlay" over its right side under
 * the header (medium and small windows), or "sheet" in place of the thread (phone).
 */
export type PaneMode = "docked" | "overlay" | "sheet";

const TAB_ID = "context-tab-";

export function SurfacePane({
  thread,
  mode,
  closing,
  moves,
  maximized,
  details = null,
  onToggleMaximize,
  onReturn,
}: {
  thread: Thread;
  mode: PaneMode;
  closing: boolean;
  /** False while the pane is part of the page as it first appeared. */
  moves: boolean;
  maximized: boolean;
  /** The thread's details card, which sits at the top of the panel above its tabs. */
  details?: ReactNode;
  onToggleMaximize(): void;
  onReturn(): void;
}) {
  const order = useUi((state) => state.panelTabOrder);
  const closedTabs = useUi((state) => state.panelTabsClosed);
  const rowIds = order.filter((id) => !closedTabs.includes(id));
  // With no surface chosen yet, or the chosen one closed, the panel shows the first in the row.
  const chosen = useUi((state) => state.panelTab);
  const tab: RightTab = chosen && rowIds.includes(chosen) ? chosen : (rowIds[0] ?? "diff");
  const toggle = useUi((state) => state.togglePanel);
  const closeTab = useUi((state) => state.closePanelTab);
  const reopenTab = useUi((state) => state.reopenPanelTab);
  const moveTab = useUi((state) => state.movePanelTab);
  const resetTabs = useUi((state) => state.resetPanelTabs);
  const width = useUi((state) => state.panelWidth);
  // A header row of its own, with Back to thread: the phone's sheet, and the reading layout's floating drawer.
  const reading = useUi((state) => state.workspaceLayout === "reading");
  const drawer = mode === "sheet" || (reading && mode === "docked");

  const list = useRef<HTMLDivElement>(null);
  const kind = useConnectionKind();
  // When the row is narrow, the tabs that don't fit fold into More; the open tab always stays.
  const widths = useRef(new Map<RightTab, number>());
  const [shown, setShown] = useState<readonly RightTab[]>(() => rowIds);
  const [plusMenu, setPlusMenu] = useState<{ x: number; y: number } | null>(null);
  const [moreMenu, setMoreMenu] = useState<{ x: number; y: number } | null>(null);
  const [dragging, setDragging] = useState<RightTab | null>(null);
  const [over, setOver] = useState<RightTab | null>(null);
  const folded = rowIds.some((id) => !shown.includes(id));
  const changed = tabsChanged(order, closedTabs);
  const latest = useRef({ tab, folded, ids: rowIds });
  latest.current = { tab, folded, ids: rowIds };
  const refit = useCallback(() => {
    const row = list.current;
    if (!row) return;
    // A tab is wider while it is the open one, so only the widths of tabs that are not open are kept for later.
    const measured = new Map(widths.current);
    for (const element of row.querySelectorAll<HTMLElement>("[data-tab]")) {
      const id = element.dataset.tab as RightTab;
      if (element.offsetWidth <= 0) continue;
      if (id !== latest.current.tab) widths.current.set(id, element.offsetWidth);
      measured.set(id, element.offsetWidth);
    }
    // What the tabs may take: the header less its padding, the "+" and the buttons at the end (the More button among them while it shows).
    const head = row.parentElement;
    const end = head?.querySelector<HTMLElement>(".head-end");
    // A header with no width yet (not laid out, or hidden) folds nothing.
    const room = !head || head.clientWidth === 0 ? Number.POSITIVE_INFINITY : head.clientWidth - HEAD_PAD - PLUS_SPACE - (end?.offsetWidth ?? 0) + (latest.current.folded ? MORE_WIDTH : 0);
    const next = fitTabs(latest.current.ids, measured, room, MORE_WIDTH, latest.current.tab);
    setShown((current) => (current.length === next.length && current.every((id, i) => id === next[i]) ? current : next));
  }, []);
  useLayoutEffect(() => {
    const row = list.current;
    if (!row) return;
    refit();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(refit);
    observer?.observe(row);
    return () => observer?.disconnect();
  }, [refit]);
  const rowKey = rowIds.join();
  useLayoutEffect(refit, [tab, rowKey, refit]);

  const reset: MenuItem = { label: "Reset tab order", icon: <RefreshCw size={14} aria-hidden />, separatorBefore: true, onSelect: resetTabs };
  const hiddenTabs: MenuItem[] = [
    ...rowIds
      .filter((id) => !shown.includes(id))
      .map((id) => {
        const { label, icon: Icon } = tabOf(id);
        return { label, icon: <Icon size={14} aria-hidden />, onSelect: () => toggle(id) };
      }),
    ...(changed ? [reset] : []),
  ];
  // The "+" lists the closed surfaces, and Reset tab order once the row has changed.
  const closedItems: MenuItem[] = [
    ...order
      .filter((id) => closedTabs.includes(id))
      .map((id) => {
        const { label, icon: Icon } = tabOf(id);
        return { label, icon: <Icon size={14} aria-hidden />, onSelect: () => reopenTab(id) };
      }),
    ...(closedTabs.length === 0 && !changed ? [{ label: "All tabs are open.", disabled: true }] : []),
    ...(changed ? [{ ...reset, separatorBefore: closedTabs.length > 0 }] : []),
  ];
  const [said, setSaid] = useState("");
  const plusButton = useRef<HTMLButtonElement>(null);
  const focusTab = (id: RightTab) =>
    requestAnimationFrame(() => list.current?.querySelector<HTMLElement>(`#${TAB_ID}${id}`)?.focus());
  /** Closes a tab, then moves focus by the shared rule: the neighbouring tab, else the "+", never the page. */
  const closeWithFocus = (id: RightTab) => {
    const at = rowIds.indexOf(id);
    const neighbour = rowIds[at + 1] ?? rowIds[at - 1];
    const target =
      (neighbour ? list.current?.querySelector<HTMLElement>(`#${TAB_ID}${neighbour}`) : null) ?? plusButton.current ?? list.current;
    const back = captureFocusReturn(target);
    flushSync(() => closeTab(id));
    back.restore();
    setSaid("Tab closed");
  };
  // One Tab stop: the chosen tab. Arrows move between tabs, Delete closes one, Alt+Shift+Arrow moves it.
  const onTabKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = rowIds.indexOf(tab);
    const current = (event.target as HTMLElement).closest<HTMLElement>("[data-tab]")?.dataset.tab as RightTab | undefined;
    if (!current) return;
    if (event.altKey && event.shiftKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      const neighbour = rowIds[rowIds.indexOf(current) + (event.key === "ArrowLeft" ? -1 : 1)];
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
      event.key === "ArrowRight" ? (at + 1) % rowIds.length
      : event.key === "ArrowLeft" ? (at - 1 + rowIds.length) % rowIds.length
      : event.key === "Home" ? 0
      : event.key === "End" ? rowIds.length - 1
      : -1;
    const target = rowIds[next];
    if (!target) return;
    event.preventDefault();
    toggle(target);
    focusTab(target);
  };
  const onDrop = (event: DragEvent<HTMLDivElement>, onto: RightTab) => {
    event.preventDefault();
    if (dragging && dragging !== onto) {
      moveTab(dragging, order.indexOf(onto));
      setSaid("Tab moved");
    }
    setDragging(null);
    setOver(null);
  };

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
      {mode === "sheet" || maximized ? null : <PaneResizer />}
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
      {details}
      <header className="context-head">
        <div ref={list} className="tabs" role="tablist" aria-label="Context" onKeyDown={onTabKey}>
          {rowIds.filter((id) => shown.includes(id)).map((id) => {
            const { label, icon: Icon } = tabOf(id);
            const offline = kind === "offline" && NEEDS_NETWORK.includes(id);
            return (
              <div
                key={id}
                className="ptab"
                data-tab={id}
                data-selected={tab === id || undefined}
                data-dragging={dragging === id || undefined}
                data-over={over === id && dragging !== id ? "" : undefined}
                data-offline={offline ? "" : undefined}
                draggable
                onDragStart={(event) => {
                  setDragging(id);
                  event.dataTransfer?.setData("text/plain", id);
                }}
                onDragEnd={() => {
                  setDragging(null);
                  setOver(null);
                }}
                onDragOver={(event) => {
                  if (!dragging) return;
                  event.preventDefault();
                  setOver(id);
                }}
                onDragLeave={(event) => {
                  if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver((current) => (current === id ? null : current));
                }}
                onDrop={(event) => onDrop(event, id)}
              >
                <button
                  type="button"
                  role="tab"
                  id={`${TAB_ID}${id}`}
                  aria-label={label}
                  aria-selected={tab === id}
                  tabIndex={tab === id ? 0 : -1}
                  aria-controls={tab === id ? `context-${id}` : undefined}
                  onClick={() => {
                    if (tab !== id) toggle(id);
                  }}
                >
                  <Icon size={14} aria-hidden />
                  <span className="ptl">{label}</span>
                  {offline ? <CloudOff size={12} aria-hidden /> : null}
                </button>
                {rowIds.length > 1 ? (
                  <button type="button" className="ptx" tabIndex={-1} aria-label={`Close ${label} tab`} title={`Close ${label}`} onClick={() => closeWithFocus(id)}>
                    <X size={11} aria-hidden />
                  </button>
                ) : null}
              </div>
            );
          })}
        </div>
        {(
          <IconButton
            ref={plusButton}
            className="padd"
            label="Open a tab"
            pressed={plusMenu !== null}
            aria-haspopup="menu"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setPlusMenu(plusMenu ? null : { x: rect.right - 200, y: rect.bottom + 4 });
            }}
          >
            <Plus size={14} aria-hidden />
          </IconButton>
        )}
        <div className="head-end">
          {folded ? (
            <IconButton
              label="More tabs"
              pressed={moreMenu !== null}
              aria-haspopup="menu"
              onClick={(event) => {
                const rect = event.currentTarget.getBoundingClientRect();
                setMoreMenu(moreMenu ? null : { x: rect.right - MORE_MENU_WIDTH, y: rect.bottom + 4 });
              }}
            >
              <MoreHorizontal size={16} aria-hidden />
            </IconButton>
          ) : null}
          {drawer ? null : (
            <>
              <IconButton label={maximized ? "Restore" : "Maximise panel"} onClick={onToggleMaximize}>
                {maximized ? <Minimize2 size={16} aria-hidden /> : <Maximize2 size={16} aria-hidden />}
              </IconButton>
              <IconButton label="Close side panel" onClick={onReturn}>
                <X size={16} aria-hidden />
              </IconButton>
            </>
          )}
        </div>
      </header>
      <Presence>
        {plusMenu ? (
          <Menu x={plusMenu.x} y={plusMenu.y} label="Open a tab" items={closedItems} onClose={() => setPlusMenu(null)} />
        ) : null}
      </Presence>
      <Presence>
        {moreMenu ? (
          <Menu
            x={moreMenu.x}
            y={moreMenu.y}
            label="Hidden tabs"
            heading="Hidden tabs"
            items={hiddenTabs}
            onClose={() => setMoreMenu(null)}
          />
        ) : null}
      </Presence>
      <div className="context-body" data-connection={kind ?? undefined}>
        {kind === "offline" && tab !== "browser" && NEEDS_NETWORK.includes(tab) ? (
          <p className="conn-banner conn-panel-banner" role="status">
            <CloudOff size={13} aria-hidden />
            <span>You’re offline.</span>
          </p>
        ) : null}
        {kind === "service-down" && tab !== "browser" ? (
          <div className="conn-wait">
            <b>Waiting for the Orbyn service</b>
            {WAITING_LINE[tab] ? <span>{WAITING_LINE[tab]}</span> : null}
          </div>
        ) : (
          <div id={`context-${tab}`} role="tabpanel" aria-labelledby={`${TAB_ID}${tab}`} className="tabpanel">
            {tab === "browser" ? (
              <BrowserPanel threadId={thread.id} />
            ) : tab === "diff" ? (
              <DiffPanel threadId={thread.id} />
            ) : tab === "files" ? (
              <FilesPanel threadId={thread.id} />
            ) : tab === "activity" ? (
              <ActivityPanel key={thread.id} threadId={thread.id} />
            ) : (
              <SideChatPane parent={thread} />
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
