import { ChevronRight, FilePen, Plus, ShieldAlert, X } from "lucide-react";
import type { ReactNode } from "react";
import type { Project } from "../../../api/types";
import { DRAFT_KEY, useDraftStore } from "../../../product/state/draftStore";
import { routeHref, type Route } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { ProjectBadge } from "../../../product/ui/badges";
import { deriveProjectIdentity } from "../../../product/projects/projectIdentity";
import { Collapsible } from "../../../ui/Collapsible";
import type { DropSection } from "./groups";
import { pushUndo } from "./threadUndo";

/** A project with no threads yet still needs a way in, and its trust state shown. */
export function EmptyProjectRow({ project }: { project: Project }) {
  const target: Route = project.trusted
    ? { name: "project", projectId: project.id }
    : { name: "trust", projectId: project.id, threadId: null };
  return (
    <li>
      <a
        className="thread-row"
        href={routeHref(target)}
        aria-label={project.trusted ? `Open ${project.name}` : `Review trust for ${project.name}`}
      >
        <ProjectBadge name={project.name} project={project} />
        <span className="thread-row-title">{project.name}</span>
        {/* One status at a time: trust comes first, and the row has room for one. */}
        {project.trusted ? (
          <span className="thread-row-empty">No threads yet</span>
        ) : (
          <span className="row-status tone-warn" title="Folder not trusted">
            <ShieldAlert size={14} aria-hidden />
            Not trusted
          </span>
        )}
      </a>
    </li>
  );
}

/** The unsent draft, reachable from anywhere; discarding it keeps it for the undo. */
export function DraftRow({ route }: { route: Route }) {
  const text = useUi((state) => state.drafts[DRAFT_KEY] ?? "");
  const projectId = useUi((state) => state.draftProject);
  const files = useDraftStore((state) => state.files);
  if (route.name === "new" || route.name === "home" || (!text.trim() && files.length === 0)) return null;
  const discard = () => {
    const { setDraft } = useUi.getState();
    const { setFiles } = useDraftStore.getState();
    setDraft(DRAFT_KEY, "");
    setFiles([]);
    pushUndo("Discarded", async () => {
      setDraft(DRAFT_KEY, text);
      setFiles(files);
    });
  };
  return (
    <li className="draft-row">
      <a className="thread-row" href={routeHref({ name: "new", projectId })}>
        <FilePen size={14} aria-hidden />
        <span className="thread-row-title">{text.trim().split("\n")[0] || `${files.length} files`}</span>
        <span className="row-trailing">
          <span className="row-rest">
            <span className="row-age">Unsent</span>
          </span>
          <span className="row-actions">
            <button
              type="button"
              className="row-action"
              aria-label="Discard draft"
              title="Discard draft"
              onClick={(event) => {
                event.preventDefault();
                discard();
              }}
            >
              <X size={14} aria-hidden />
            </button>
          </span>
        </span>
      </a>
    </li>
  );
}

export function Group({
  id,
  label,
  count,
  defaultOpen,
  dropSection,
  forceOpen,
  children,
}: {
  id: string;
  label: string;
  count: number;
  defaultOpen: boolean;
  dropSection?: DropSection;
  forceOpen?: boolean;
  children: ReactNode;
}) {
  const open = useUi((state) => state.groups[id] ?? defaultOpen);
  const toggle = useUi((state) => state.toggleGroup);
  return (
    <div className="group" data-drop-section={dropSection ?? "none"}>
      <Collapsible
        open={open || forceOpen === true}
        onOpenChange={() => toggle(id, defaultOpen)}
        summaryClassName="group-head"
        summary={
          <>
            <span>{label}</span>
            <span className="group-count">{count}</span>
            <ChevronRight size={14} className="chev" aria-hidden />
          </>
        }
      >
        <ul className="thread-list group-list">{children}</ul>
      </Collapsible>
    </div>
  );
}

export function DragBoundary({ label, section, active }: { label: string; section: DropSection; active: boolean }) {
  return (
    <li className="drag-boundary" data-drop-section={section} data-active={active || undefined}>
      <span>{label}</span>
    </li>
  );
}

export function DropPlaceholder({ label, section, active }: { label: string; section: DropSection; active: boolean }) {
  return (
    <li className="drop-placeholder" data-drop-section={section} data-active={active || undefined}>
      {label}
    </li>
  );
}

export function FolderHeader({
  project,
  label,
  collapsed,
  count,
  needs,
  onToggle,
  onMenu,
}: {
  project: Project;
  label: string;
  collapsed: boolean;
  count: number;
  /** Threads in the folder that wait on the person. */
  needs: number;
  onToggle(): void;
  onMenu(event: React.MouseEvent): void;
}) {
  return (
    <li className="folder-head" data-count={needs > 0 || (collapsed && count > 0) ? "" : undefined}>
      <button
        type="button"
        className="folder-toggle"
        aria-expanded={!collapsed}
        onClick={onToggle}
        onContextMenu={(event) => {
          event.preventDefault();
          onMenu(event);
        }}
      >
        <ChevronRight size={12} className="chev folder-chev" aria-hidden />
        {project.project_icon ? (
          <ProjectBadge name={project.name} project={project} />
        ) : (
          <span className={`project-swatch project-color-${deriveProjectIdentity(project.name).color}`} aria-hidden />
        )}
        <span className="folder-name">{label}</span>
        {needs > 0 ? (
          <span className="folder-needs">
            <span aria-hidden>{needs}</span>
            <span className="sr-only">{needs} waiting on you</span>
          </span>
        ) : null}
        {collapsed && count > 0 ? <span className="group-count">{count}</span> : null}
      </button>
      <a
        className="icon-btn folder-new"
        href={routeHref({ name: "new", projectId: project.id })}
        aria-label={`New thread in ${project.name}`}
        title={`New thread in ${project.name}`}
      >
        <Plus size={14} aria-hidden />
      </a>
    </li>
  );
}
