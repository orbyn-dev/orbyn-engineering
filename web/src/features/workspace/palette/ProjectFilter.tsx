import { Check, Folder, Settings } from "lucide-react";
import { SlidersIcon } from "./SlidersIcon";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Project } from "../../../api/types";
import { navigate } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { ProjectBadge } from "../../../product/ui/badges";
import { usePopover } from "../../../ui/usePopover";

export const POPUP_WIDTH = 288;
/** The popup's tallest size (max-height in shell.css), which the vertical clamp keeps inside the window. */
export const POPUP_HEIGHT = 320;
/** The popup under its trigger, left edges aligned, pulled back in when the window is too small for it. */
export function popupPlace(
  trigger: { left: number; bottom: number },
  window: { width: number; height: number },
): { left: number; top: number } {
  return {
    left: Math.max(8, Math.min(trigger.left, window.width - POPUP_WIDTH - 8)),
    top: Math.max(8, Math.min(trigger.bottom + 4, window.height - POPUP_HEIGHT - 8)),
  };
}

/** Limits the thread list to one or several projects. Shift adds or removes one. */
export function ProjectFilter({ projects }: { projects: readonly Project[] }) {
  const filter = useUi((state) => state.projectFilter);
  const setPrefs = useUi((state) => state.setPrefs);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  // Where the popup sits: under its trigger, kept inside the window. Fixed, so no ancestor decides it.
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const popover = usePopover({ kind: "fields", root: popup, active: open, trigger: () => trigger.current, onClose: () => setOpen(false) });

  // The popup follows its trigger when the window is resized or anything scrolls (the sidebar list under it, say).
  useEffect(() => {
    if (!open) return;
    const follow = () => {
      const box = trigger.current?.getBoundingClientRect();
      if (box) setPlace(popupPlace(box, { width: window.innerWidth, height: window.innerHeight }));
    };
    window.addEventListener("resize", follow);
    window.addEventListener("scroll", follow, true);
    return () => {
      window.removeEventListener("resize", follow);
      window.removeEventListener("scroll", follow, true);
    };
  }, [open]);

  const scoped = projects.filter((project) => filter.includes(project.id));
  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? projects.filter((project) => project.name.toLowerCase().includes(needle)) : projects;
  }, [projects, query]);
  const items: (Project | null)[] = query.trim() ? [...matches] : [null, ...matches];

  const toggle = (id: string) =>
    setPrefs({
      projectFilter: filter.includes(id) ? filter.filter((entry) => entry !== id) : [...filter, id],
    });
  const pick = (project: Project | null, additive: boolean) => {
    if (!project) {
      setPrefs({ projectFilter: [] });
    } else if (additive) {
      toggle(project.id);
      return;
    } else {
      setPrefs({ projectFilter: [project.id] });
    }
    popover.dismiss("select");
  };
  const projectSettings = () => {
    popover.dismiss("select");
    navigate({ name: "settings", section: "projects" });
  };

  const label =
    scoped.length > 0
      ? `Filter threads by project: ${scoped.map((project) => project.name).join(", ")}`
      : "Filter threads by project";

  return (
    <div className="project-filter">
      <button
        ref={trigger}
        type="button"
        className="icon-btn"
        aria-label={label}
        title={label}
        aria-expanded={open}
        onClick={() => {
          const box = trigger.current?.getBoundingClientRect();
          if (box) setPlace(popupPlace(box, { width: window.innerWidth, height: window.innerHeight }));
          setOpen(!open);
          setQuery("");
          setAt(0);
        }}
      >
        {scoped.length > 1 ? (
          <span className="filter-stack">
            {scoped.slice(0, 2).map((project) => (
              <ProjectBadge key={project.id} name={project.name} project={project} />
            ))}
          </span>
        ) : scoped[0] ? (
          <ProjectBadge name={scoped[0].name} project={scoped[0]} />
        ) : (
          <SlidersIcon size={14} />
        )}
      </button>
      {open ? (
        <div className="filter-popup" ref={popup} style={place ?? undefined}>
          <input
            autoFocus
            aria-label="Search projects"
            placeholder="Search projects..."
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setAt(0);
            }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const step = event.key === "ArrowDown" ? 1 : -1;
                setAt((current) => Math.min(items.length - 1, Math.max(0, current + step)));
              } else if (event.key === "Enter") {
                event.preventDefault();
                const item = items[at];
                if (item !== undefined) pick(item, event.shiftKey);
              } else if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
                event.preventDefault();
                if (items[at]) projectSettings();
              }
            }}
          />
          {items.length === 0 ? <p className="filter-empty">No matching projects.</p> : null}
          <div role="listbox" aria-multiselectable aria-label="Projects">
            {items.map((project, index) => {
              const selected = project ? filter.includes(project.id) : filter.length === 0;
              return (
                <div
                  key={project?.id ?? "all"}
                  role="option"
                  aria-selected={selected}
                  className="filter-item"
                  data-active={index === at || undefined}
                  onPointerEnter={() => setAt(index)}
                  onClick={(event) => pick(project, event.shiftKey)}
                  onContextMenu={(event) => {
                    if (!project) return;
                    event.preventDefault();
                    projectSettings();
                  }}
                >
                  <span className="filter-icon">
                    {selected ? (
                      <Check size={14} aria-hidden />
                    ) : project ? (
                      <ProjectBadge name={project.name} project={project} />
                    ) : (
                      <Folder size={16} aria-hidden />
                    )}
                  </span>
                  <span className="filter-name">{project?.name ?? "All projects"}</span>
                  {project ? (
                    <button
                      type="button"
                      className="row-action"
                      tabIndex={-1}
                      aria-hidden
                      title={`Project settings for ${project.name}`}
                      onClick={(event) => {
                        event.stopPropagation();
                        projectSettings();
                      }}
                    >
                      <Settings size={14} aria-hidden />
                    </button>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}
