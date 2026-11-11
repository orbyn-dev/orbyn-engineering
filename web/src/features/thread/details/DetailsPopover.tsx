import { useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import type { Thread } from "../../../api/types";
import { useLayoutWidth } from "../../../lib/useLayout";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { detailsMenuItems } from "../../../product/ui/details";
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { Button } from "../../../ui/controls";
import { usePopover } from "../../../ui/usePopover";
import { ToolsRow } from "../../tools";
import { BranchRow } from "../GitControl";
import { OpenInEditor } from "../OpenInEditor";
import { PullsRow } from "../PullChip";
import { ThreadAutomations } from "../ThreadAutomations";
import { DetailsDismiss } from "./DetailsDismiss";
import { ChangesRow, LimitsRow, ScriptsRow, ServersRow, UsageRow, useDetailsLoads } from "./DetailsRows";

/** How far down the grabber or header is dragged, in px, to dismiss the sheet. */
const DRAG_DISMISS = 64;

/** The one trigger the popover opens from, and the button it returns focus to. */
const trigger = () => document.querySelector<HTMLElement>("[data-details-toggle]");

/** The row that stands where the ones that could not be read would be. */
function ReadError({ onRetry }: { onRetry(): void }) {
  return (
    <div className="dbanner" role="alert">
      <span>Couldn’t read the details.</span>
      <span className="sp" />
      <Button small tone="quiet" onClick={onRetry}>
        Try again
      </Button>
    </div>
  );
}

/**
 * The thread's details, in the one place they live: a popover under the thread header's Details button. It is a sheet from the
 * bottom on a phone. One row for each setting turned on in Settings › General › Rows; a row with nothing to show says so rather than
 * disappearing. Escape or a press outside closes it and focus returns to the button. A right-click opens the Rows menu.
 */
export function DetailsPopover({ thread, onClose }: { thread: Thread; onClose(): void }) {
  const root = useRef<HTMLDivElement>(null);
  const layout = useLayoutWidth();
  const hidden = useUi((state) => state.detailsHidden);
  const shows = (row: string) => !hidden.includes(row);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const { dismiss } = usePopover({ kind: "fields", root, trigger, onClose });
  const loads = useDetailsLoads(thread.id, shows);
  const projectBranch = useWorkspace((state) => state.managed.data?.targets.find((t) => t.project_id === thread.project_id)?.branch ?? null);
  const branch = thread.worktree?.branch ?? projectBranch;

  // Anchored to the button's right edge, whatever the window's width.
  useLayoutEffect(() => {
    const element = root.current;
    const button = trigger();
    const host = element?.offsetParent;
    if (!element || !button || !host || layout === "phone") return;
    const at = button.getBoundingClientRect();
    const room = host.getBoundingClientRect();
    if (at.width > 0 && room.width > 0) element.style.right = `${Math.max(12, Math.round(room.right - at.right))}px`;
    else element.style.removeProperty("right");
  }, [layout]);

  // The sheet is dragged down by its grabber or its header to put it away; Escape and a press outside still do.
  const dragFrom = useRef<number | null>(null);
  const sheet = layout === "phone";
  const dragHandlers = sheet
    ? {
        onPointerDown: (event: PointerEvent) => {
          dragFrom.current = event.clientY;
          (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
        },
        onPointerMove: (event: PointerEvent) => {
          if (dragFrom.current === null || !root.current) return;
          root.current.style.transform = `translateY(${Math.max(0, event.clientY - dragFrom.current)}px)`;
        },
        onPointerUp: (event: PointerEvent) => {
          const from = dragFrom.current;
          dragFrom.current = null;
          root.current?.style.removeProperty("transform");
          if (from !== null && event.clientY - from >= DRAG_DISMISS) dismiss("escape");
        },
        onPointerCancel: () => {
          dragFrom.current = null;
          root.current?.style.removeProperty("transform");
        },
      }
    : {};

  /** The error row takes the place of the first row that failed; the rest of what failed goes with it. */
  const read = (row: string, node: ReactNode) => {
    if (!loads.failed.includes(row)) return shows(row) ? node : null;
    return loads.failed[0] === row ? <ReadError key="read-error" onRetry={loads.retry} /> : null;
  };

  return (
    <div ref={root} className="details-popover" role="dialog" aria-label="Thread details" data-sheet={sheet || undefined}>
      <span className="details-grab" aria-hidden {...dragHandlers} />
      <DetailsDismiss.Provider value={() => dismiss("select")}>
        <div
          className="dcard"
          role="group"
          onContextMenu={(event) => {
            event.preventDefault();
            setMenu({ x: event.clientX, y: event.clientY });
          }}
        >
          <Presence>{menu ? <Menu {...menu} label="Thread details rows" items={detailsMenuItems()} onClose={() => setMenu(null)} /> : null}</Presence>
          <div className="dh" {...dragHandlers}>
            Details
          </div>
          {branch && shows("branch") ? <BranchRow thread={thread} branch={branch} /> : null}
          {read("changes", <ChangesRow diff={loads.diff} />)}
          {shows("pulls") ? <PullsRow thread={thread} /> : null}
          {shows("automations") ? <ThreadAutomations threadId={thread.id} /> : null}
          {shows("tools") ? <ToolsRow thread={thread} onOpen={() => dismiss("select")} /> : null}
          {read("servers", <ServersRow threadId={thread.id} servers={loads.servers} />)}
          {shows("limits") ? <LimitsRow threadId={thread.id} engine={thread.engine} /> : null}
          {shows("usage") ? <UsageRow threadId={thread.id} /> : null}
          {read("scripts", <ScriptsRow threadId={thread.id} scripts={loads.scripts} />)}
          {shows("editor") ? <OpenInEditor threadId={thread.id} /> : null}
        </div>
      </DetailsDismiss.Provider>
    </div>
  );
}
