import { useEffect, useMemo, useState } from "react";
import { api, queryProjectScopes } from "../api";
import type { Project, ServerSettings, Thread } from "../../../api/types";
import type { ProjectGroupingMode } from "../../../generated/ProjectGroupingMode";
import type { ProjectScopeCatalog } from "../../../generated/ProjectScopeCatalog";
import { readKept, writeKept } from "../../../lib/keptData";

/** True while Mod is held on its own, when number badges show on the first rows. */
export function useJumpHeld() {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const sync = (event: KeyboardEvent) =>
      setHeld((/mac/i.test(navigator.platform) ? event.metaKey : event.ctrlKey) && !event.shiftKey && !event.altKey);
    const clear = () => setHeld(false);
    window.addEventListener("keydown", sync);
    window.addEventListener("keyup", sync);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", sync);
      window.removeEventListener("keyup", sync);
      window.removeEventListener("blur", clear);
    };
  }, []);
  return held;
}

export const searchFields = (thread: Thread, project: string) =>
  `${thread.title} ${project} ${thread.worktree?.branch ?? ""}`.toLowerCase();

const SETTINGS_KEY = "sidebar:settings";
const CATALOG_KEY = "sidebar:grouping-catalog";

export function useSidebarCatalog(
  list: Project[],
  projectFolders: boolean,
  projectGrouping: ProjectGroupingMode,
  projectGroupingOverrides: Record<string, ProjectGroupingMode>,
) {
  const groupingOverrides = useMemo(
    () =>
      Object.entries(projectGroupingOverrides)
        .filter(([id]) => list.some((project) => project.id === id))
        .map(([project_id, mode]) => ({ project_id, mode }))
        .sort((a, b) => a.project_id.localeCompare(b.project_id)),
    [list, projectGroupingOverrides],
  );

  // The polled project list gives `groupingOverrides` a new identity every few seconds; the
  // catalog is asked again only when its content changes.
  const overridesKey = JSON.stringify(groupingOverrides);
  // A renamed, added or re-iconed project changes the catalog's labels and members too.
  const projectsKey = list.map((project) => `${project.id}:${project.name}:${project.metadata_revision ?? ""}`).join("|");
  const catalogKey = `${CATALOG_KEY}:${projectFolders}:${projectGrouping}:${overridesKey}:${projectsKey}`;
  // A sidebar that mounts again (a drawer, reading mode) draws what it last read at once.
  const [settings, setSettings] = useState<ServerSettings | null>(() => readKept<ServerSettings>(SETTINGS_KEY) ?? null);
  const [groupingCatalog, setGroupingCatalog] = useState<ProjectScopeCatalog | null>(() => readKept<ProjectScopeCatalog>(catalogKey) ?? null);
  useEffect(() => {
    const abort = new AbortController();
    api.settings(abort.signal).then((read) => {
      writeKept(SETTINGS_KEY, read);
      setSettings(read);
    }, () => undefined);
    return () => abort.abort();
  }, []);
  const presets = useMemo(() => settings?.snooze_presets ?? [], [settings]);
  useEffect(() => {
    // Separate grouping with no overrides is one folder per project, which needs no catalog.
    if (!projectFolders || (projectGrouping === "separate" && overridesKey === "[]")) {
      setGroupingCatalog(null);
      return;
    }
    const abort = new AbortController();
    queryProjectScopes(
      { recipe: { mode: projectGrouping, overrides: JSON.parse(overridesKey) as typeof groupingOverrides } },
      abort.signal,
    ).then((read) => {
      writeKept(catalogKey, read);
      setGroupingCatalog(read);
    }, () => {
      if (!abort.signal.aborted) setGroupingCatalog(null);
    });
    return () => abort.abort();
  }, [overridesKey, projectsKey, projectFolders, projectGrouping, catalogKey]);
  return { presets, groupingCatalog };
}

/** Clicks on anything but a row drop the selection. */
export function useSelectionDismiss(selection: ReadonlySet<string>, setSelection: (next: ReadonlySet<string>) => void) {
  useEffect(() => {
    if (selection.size === 0) return;
    const clear = (event: PointerEvent) => {
      if (!(event.target as Element | null)?.closest?.("[data-thread-item], [data-menu-root]")) {
        setSelection(new Set());
      }
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSelection(new Set());
    };
    document.addEventListener("pointerdown", clear);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", clear);
      document.removeEventListener("keydown", key);
    };
  }, [selection, setSelection]);
}
