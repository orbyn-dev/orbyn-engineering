import { ChartColumn, Clock, GitPullRequest, Settings, SquarePen } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import { routeHref, type Route } from "../../../state/route";
import { DEFAULT_SIDEBAR_WIDTH, useUi } from "../../../state/ui";
import { UpdatePill } from "../app-chrome/Updates";
import { SkillsNav } from "../../skills";
import { undoLabel, undoLatest, useUndoNotice } from "./threadUndo";

const MIN_WIDTH = 180;
const MAX_WIDTH = 420;
const clampWidth = (width: number) => Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(width)));

/** Resizes live through a CSS variable; the preference saves when the drag ends. */
export function SidebarRail() {
  const width = useUi((state) => state.sidebarWidth);
  const setWidth = useUi((state) => state.setSidebarWidth);
  const [dragging, setDragging] = useState(false);
  const latest = useRef(width);
  const preview = (next: number) =>
    document.querySelector<HTMLElement>(".shell")?.style.setProperty("--sidebar-w", `${next}px`);

  return (
    <div
      className="sidebar-rail"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      aria-valuenow={width}
      aria-valuemin={MIN_WIDTH}
      aria-valuemax={MAX_WIDTH}
      tabIndex={0}
      data-dragging={dragging || undefined}
      title="Drag to resize"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        latest.current = width;
        setDragging(true);
      }}
      onPointerMove={(event) => {
        if (!dragging) return;
        latest.current = clampWidth(event.clientX);
        preview(latest.current);
      }}
      onLostPointerCapture={() => {
        setDragging(false);
        setWidth(latest.current);
      }}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        setWidth(clampWidth(width + (event.key === "ArrowRight" ? 16 : -16)));
      }}
      onDoubleClick={() => setWidth(DEFAULT_SIDEBAR_WIDTH)}
    />
  );
}

function UndoNotice() {
  const notice = useUndoNotice((state) => state.notice);
  if (!notice) return null;
  const summary = notice.count === 1 ? notice.action : `${notice.action} ${notice.count}`;
  return (
    <div className="undo-notice" role="status">
      <span>{summary},</span>
      <button type="button" onClick={() => void undoLatest()}>
        {undoLabel()}
      </button>
    </div>
  );
}

const navRows: { label: string; icon: ReactNode; route: Route; current(route: Route): boolean }[] = [
  {
    label: "New thread",
    icon: <SquarePen size={15} aria-hidden />,
    route: { name: "new", projectId: null },
    current: (route) => route.name === "new" || route.name === "home",
  },
  {
    label: "Pull requests",
    icon: <GitPullRequest size={15} aria-hidden />,
    route: { name: "pulls" },
    current: (route) => route.name === "pulls",
  },
  {
    label: "Usage",
    icon: <ChartColumn size={15} aria-hidden />,
    route: { name: "usage" },
    current: (route) => route.name === "usage",
  },
  {
    label: "Scheduled tasks",
    icon: <Clock size={15} aria-hidden />,
    route: { name: "tasks" },
    current: (route) => route.name === "tasks" || (route.name === "settings" && route.section === "tasks"),
  },
];

function NavRow({
  label,
  icon,
  route,
  current,
}: {
  label: string;
  icon: ReactNode;
  route: Route;
  current: boolean;
}) {
  return (
    <a className="nav-row" href={routeHref(route)} aria-current={current ? "page" : undefined}>
      {icon}
      <span className="nav-row-label">{label}</span>
    </a>
  );
}

/** The labelled places to go. Console and Reading list them under the search row; Ledger at the bottom. */
export function SidebarNav({ route, ledger }: { route: Route; ledger: boolean }) {
  return (
    <div className="sidebar-nav" data-ledger={ledger || undefined}>
      {navRows.map((row) => (
        <NavRow key={row.label} label={row.label} icon={row.icon} route={row.route} current={row.current(route)} />
      ))}
      <SkillsNav route={route} />
    </div>
  );
}

export function SidebarFoot({ route, ledger }: { route: Route; ledger: boolean }) {
  return (
    <div className="sidebar-foot">
      <UndoNotice />
      <UpdatePill />
      {ledger ? <SidebarNav route={route} ledger /> : null}
      <NavRow
        label="Settings"
        icon={<Settings size={15} aria-hidden />}
        route={{ name: "settings", section: null }}
        current={route.name === "settings" && route.section !== "tasks"}
      />
    </div>
  );
}
