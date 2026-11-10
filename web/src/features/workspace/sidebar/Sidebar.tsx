import { Plus, Search } from "lucide-react";
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Thread } from "../../../api/types";
import type { ProjectGroupingMode } from "../../../generated/ProjectGroupingMode";
import { useDraftStore } from "../../../product/state/draftStore";
import { navigate, type Route } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { IconButton } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { ErrorState, Loading } from "../../../product/ui/states";
import {
  arrangeThreads,
  attentionRank,
  dropVerb,
  foldersOf,
  projectDrop,
  sectionOf,
  sweepIds,
  drawnOrder,
  ledgerGroupsOf,
  ledgerKey,
  type DropTarget,
  type DropVerb,
  type ProjectFolder,
  type SidebarSection,
} from "./groups";
import { trackPress } from "./pressGesture";
import { ProjectFilter } from "../palette/ProjectFilter";
import {
  deleteThreads,
  dismissWoke,
  settleThreads,
  unpinThreads,
  unsettleThreads,
  wakeThreads,
} from "./threadActions";
import { bulkMenuItems, CustomSnoozeDialog, RenameThreadDialog, snoozeChildren, threadMenuItems } from "./threadMenu";
import { ThreadRow, type RowActions } from "./ThreadRow";
import { useDrawnOrder } from "./drawnOrder";
import { DragBoundary, DraftRow, DropPlaceholder, EmptyProjectRow, FolderHeader, Group } from "./listRows";
import { SidebarFoot, SidebarNav, SidebarRail } from "./SidebarChrome";
import { dropTargetAt, finishDrop, mentionInComposer, sameTarget, sweepVerbs, threadIdAt, type DragState } from "./dragDrop";
import { DeleteThreadsDialog } from "./DeleteThreadsDialog";
import { SearchField } from "./ThreadSearchField";
import { folderMenuItems } from "./folderMenu";
import { handleTreeKey } from "./treeKeys";
import { searchFields, useJumpHeld, useSelectionDismiss, useSidebarCatalog } from "./sidebarHooks";

const SETTLED_PAGE = 10;

interface MenuState {
  x: number;
  y: number;
  label: string;
  items: MenuItem[];
}

export const Sidebar = memo(function Sidebar({ route }: { route: Route }) {
  const jumpHeld = useJumpHeld();
  const projects = useWorkspace((state) => state.projects);
  const visibleProjects = useVisibleProjects();
  const setPalette = useUi((state) => state.setPalette);
  const threads = useWorkspace((state) => state.threads);
  const putThread = useWorkspace((state) => state.putThread);
  const refreshAll = useWorkspace((state) => state.refreshAll);
  const threadsLoaded = useWorkspace((state) => Object.values(state.threadIds).some((ids) => ids.data));
  const threadError = useWorkspace(
    (state) => Object.values(state.threadIds).find((ids) => ids.error && !ids.data)?.error,
  );
  const toggleGroup = useUi((state) => state.toggleGroup);
  const closedGroups = useUi((state) => state.groups);
  const projectFilter = useUi((state) => state.projectFilter);
  const projectFolders = useUi((state) => state.projectFolders);
  const multilineRows = useUi((state) => state.multilineRows);
  const workingSection = useUi((state) => state.workingSection);
  const workspaceLayout = useUi((state) => state.workspaceLayout);
  const ledger = workspaceLayout === "ledger";
  // Ledger sorts by what needs the person, so it replaces the project folders.
  const threadSort = useUi((state) => state.threadSort);
  const projectSort = useUi((state) => state.projectSort);
  const projectGrouping = useUi((state) => state.projectGrouping);
  const projectGroupingOverrides = useUi((state) => state.projectGroupingOverrides);
  const setPrefs = useUi((state) => state.setPrefs);
  const pushToast = useUi((state) => state.pushToast);

  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);
  const [settledVisible, setSettledVisible] = useState(SETTLED_PAGE);
  const [selection, setSelection] = useState<ReadonlySet<string>>(new Set());
  const [anchorId, setAnchorId] = useState<string | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const renamingThread = renaming ? threads[renaming] : undefined;
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [snoozing, setSnoozing] = useState<Thread[] | null>(null);
  const [deleting, setDeletingState] = useState<Thread[] | null>(null);
  const setDeleting = (threads: Thread[]) => {
    if (useUi.getState().confirmDelete) setDeletingState(threads);
    else void deleteThreads(threads);
  };
  const [drag, setDrag] = useState<DragState | null>(null);
  const [sweep, setSweep] = useState<{ verb: DropVerb; ids: ReadonlySet<string> } | null>(null);

  const nav = useRef<HTMLElement>(null);
  const tree = useRef<HTMLDivElement>(null);
  const pointerY = useRef(0);
  const grabOffset = useRef(0);
  const selectedThread = route.name === "thread" ? route.id : null;
  const list = useMemo(() => visibleProjects ?? [], [visibleProjects]);

  const { presets, groupingCatalog } = useSidebarCatalog(list, projectFolders, projectGrouping, projectGroupingOverrides);

  const scoped = useMemo(() => {
    const all = Object.values(threads);
    return projectFilter.length === 0 ? all : all.filter((thread) => projectFilter.includes(thread.project_id));
  }, [threads, projectFilter]);

  // Ledger lists running threads in its own Running section, not on the Working shelf.
  const sections = arrangeThreads(scoped, Date.now(), workingSection && !ledger, ledger);
  const shown = drag?.target ? projectDrop(sections, drag.id, drag.target) : sections;
  // What the list draws, in order; set below, once the folders and groups are known.
  const orderedRef = useRef<Thread[]>([]);
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const sectionsRef = useRef(sections);
  sectionsRef.current = sections;

  const results = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return [];
    return scoped
      .filter((thread) => !thread.lifecycle?.archived_at && thread.lifecycle?.relation !== "side")
      .filter((thread) =>
        searchFields(thread, list.find((p) => p.id === thread.project_id)?.name ?? "").includes(needle),
      )
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  }, [scoped, list, query]);
  const isSearching = query.trim() !== "";

  const emptyProjects = list.filter(
    (project) =>
      (projectFilter.length === 0 || projectFilter.includes(project.id)) &&
      !Object.values(threads).some((thread) => thread.project_id === project.id),
  );

  useSelectionDismiss(selection, setSelection);

  const follow = useCallback(() => {
    if (!drag) return;
    const row = tree.current?.querySelector<HTMLElement>(`[data-thread-item="${CSS.escape(drag.id)}"]`);
    if (!row) return;
    row.style.transform = "";
    const top = row.getBoundingClientRect().top;
    row.style.transform = `translateY(${pointerY.current - grabOffset.current - top}px)`;
  }, [drag]);
  useLayoutEffect(follow);

  const actions: RowActions = {
    onClick(event, thread) {
      const mod = /mac/i.test(navigator.platform) ? event.metaKey : event.ctrlKey;
      if (mod) {
        event.preventDefault();
        setAnchorId(thread.id);
        setSelection((current) => {
          const next = new Set(current);
          if (!next.delete(thread.id)) next.add(thread.id);
          return next;
        });
      } else if (event.shiftKey) {
        event.preventDefault();
        const rows = orderedRef.current;
        const from = rows.findIndex((row) => row.id === (anchorId ?? selectedThread));
        const to = rows.findIndex((row) => row.id === thread.id);
        const start = from < 0 ? to : Math.min(from, to);
        const end = from < 0 ? to : Math.max(from, to);
        setSelection(new Set(rows.slice(start, end + 1).map((row) => row.id)));
      } else if (event.detail === 2) {
        event.preventDefault();
        setRenaming(thread.id);
      } else if (selection.size > 0) {
        setSelection(new Set());
      }
    },
    onPressStart(event, thread, section) {
      if (event.button !== 0 || section === "working") return;
      if (event.metaKey || event.ctrlKey || event.shiftKey) return;
      const row = event.currentTarget.closest("li");
      if (!row || !tree.current) return;
      const list = tree.current;
      grabOffset.current = event.clientY - row.getBoundingClientRect().top;
      pointerY.current = event.clientY;
      let target: DropTarget | null = null;
      // Project folders keep the drag to a composer; reordering is for the flat list and Ledger's order.
      const reorder = !foldersView;
      const reset = () => {
        row.style.transform = "";
        setDrag(null);
      };
      trackPress(event, {
        onStart: () => {
          if (reorder) setDrag({ id: thread.id, from: section, target: null });
        },
        onMove: (y) => {
          pointerY.current = y;
          if (!reorder) return;
          target = dropTargetAt(list, y, thread.id, shownRef.current);
          // Ledger draws its sections by state, so a row can only be placed inside its own.
          if (ledger && target?.section === "active") {
            const rows = sectionsRef.current.active.filter((row) => row.id !== thread.id);
            const key = ledgerKey(thread);
            const near = [rows[target.index - 1], rows[target.index]];
            const fits = near.some((row) => row !== undefined && ledgerKey(row) === key);
            if (!fits && rows.some((row) => ledgerKey(row) === key)) target = null;
          }
          setDrag((current) =>
            current && sameTarget(current.target, target) ? current : current && { ...current, target },
          );
          follow();
        },
        onEnd: (at) => {
          reset();
          const composer = document.elementFromPoint(at.x, at.y)?.closest(".composer");
          if (composer) mentionInComposer(selection.has(thread.id) ? orderedRef.current.filter((row) => selection.has(row.id)) : [thread], selectedThread);
          else if (reorder && target) void finishDrop(thread, section, target, { sectionsRef, putThread, pushToast });
        },
        onCancel: reset,
      });
    },
    onActionPress(event, thread, section) {
      if (event.button !== 0 || !tree.current) return;
      event.stopPropagation();
      const list = tree.current;
      const verb = sweepVerbs[section] ?? "settle";
      const same = (row: Thread) =>
        sectionsRef.current[section].some((entry) => entry.id === row.id);
      let ids: string[] = [];
      const sweepTo = (id: string | null) => {
        if (!id) return;
        ids = sweepIds(orderedRef.current, thread.id, id, same);
        setSweep({ verb, ids: new Set(ids) });
      };
      trackPress(event, {
        onStart: () => sweepTo(thread.id),
        onMove: (y) => sweepTo(threadIdAt(list, y)),
        onEnd: () => {
          setSweep(null);
          const swept = orderedRef.current.filter((row) => ids.includes(row.id));
          if (verb === "settle") void settleThreads(swept);
          else if (verb === "unsettle") void unsettleThreads(swept);
          else void wakeThreads(swept);
        },
        onCancel: () => setSweep(null),
      });
    },
    onMenu(event, thread) {
      if (selection.has(thread.id) && selection.size > 1) {
        const picked = orderedRef.current.filter((row) => selection.has(row.id));
        setMenu({
          x: event.clientX,
          y: event.clientY,
          label: `Actions for ${picked.length} threads`,
          items: bulkMenuItems(picked, presets, setSnoozing, setDeleting),
        });
        return;
      }
      setSelection(new Set());
      const project = list.find((p) => p.id === thread.project_id);
      const branch =
        thread.worktree?.branch ??
        useWorkspace.getState().managed.data?.targets.find((t) => t.project_id === thread.project_id)?.branch ??
        null;
      const filterActive = projectFilter.length > 0;
      setMenu({
        x: event.clientX,
        y: event.clientY,
        label: `Actions for ${thread.title}`,
        items: threadMenuItems({
          thread,
          branch,
          projectName: project?.name ?? "this project",
          filterActive,
          presets,
          onRename: () => setRenaming(thread.id),
          onCustomSnooze: setSnoozing,
          onDelete: setDeleting,
          onFilter: () => setPrefs({ projectFilter: filterActive ? [] : [thread.project_id] }),
        }),
      });
    },
    onSettle: (thread) => void settleThreads([thread]),
    onUnsettle: (thread) => void unsettleThreads([thread]),
    onWake: (thread) => void wakeThreads([thread]),
    onUnpin: (thread) => void unpinThreads([thread]),
    onSnooze(event, thread) {
      setMenu({
        x: event.clientX,
        y: event.clientY,
        label: "Snooze thread",
        items: snoozeChildren([thread], presets, () => setSnoozing([thread])),
      });
    },
    onRowFocus: setFocusId,
    onDismissWoke: (thread) => void dismissWoke(thread),
    onFiles(thread, files) {
      useDraftStore.getState().drop(thread.id, files);
      navigate({ name: "thread", id: thread.id });
    },
  };

  const onTreeKey = (event: React.KeyboardEvent) => handleTreeKey(event, { tree, threads, setRenaming, actions });

  const foldersView = projectFolders && !ledger;
  const settledRows = shown.settled.slice(0, Math.max(settledVisible, 1));
  const ledgerGroups = ledger ? ledgerGroupsOf(shown.active) : [];
  const folders = foldersView
    ? foldersOf(
        projectFilter.length > 0 ? list.filter((p) => projectFilter.includes(p.id)) : list,
        shown.active,
        groupingCatalog?.groups.map((group) => ({
          key: group.key,
          label: group.label,
          projectIds: group.members.map((member) => member.project_id),
        })) ?? null,
        projectSort,
        threadSort,
      )
    : [];
  // Threads outside every folder (no project, or a project the list hides) still need a row.
  const inFolders = new Set(folders.flatMap((folder) => folder.threads.map((thread) => thread.id)));
  const strays = foldersView ? shown.active.filter((thread) => !inFolders.has(thread.id)) : [];
  const shelves = shown.working;
  const groupOpen = (id: "working" | "snoozed" | "settled") => closedGroups[id] ?? id !== "settled";
  const drawn = drawnOrder({
    sections: shown,
    ledger,
    folders: foldersView ? folders : null,
    strays,
    folderOpen: (folder) => closedGroups[`folder:${folder.key}`] !== false,
    groupOpen: (id) => groupOpen(id) || drag !== null,
    settled: settledRows,
  });
  orderedRef.current = isSearching ? results : drawn;
  const jumpRank = new Map(
    orderedRef.current
      .filter((thread) => !thread.lifecycle?.settled_at)
      .slice(0, 9)
      .map((thread, at) => [thread.id, at + 1]),
  );
  const drawnIds = orderedRef.current.map((thread) => thread.id).join("\n");
  useEffect(() => {
    useDrawnOrder.getState().set(drawnIds ? drawnIds.split("\n") : []);
  }, [drawnIds]);
  useEffect(() => () => useDrawnOrder.getState().set(null), []);

  const firstRow = orderedRef.current[0]?.id ?? null;
  const rovingId =
    focusId && threads[focusId] ? focusId : selectedThread && threads[selectedThread] ? selectedThread : firstRow;

  const row = (
    thread: Thread,
    section: SidebarSection,
    compact: boolean,
    highlighted = false,
    nested = false,
    showProject = false,
  ) => {
    const dragged = drag?.id === thread.id;
    const verb = dragged
      ? dropVerb(drag.from, drag.target?.section ?? null)
      : sweep?.ids.has(thread.id)
        ? sweep.verb
        : null;
    return (
      <ThreadRow
        key={thread.id}
        jumpHint={jumpHeld ? jumpRank.get(thread.id) ?? null : null}
        thread={thread}
        section={section}
        selected={thread.id === selectedThread}
        compact={compact}
        multiline={multilineRows}
        multiSelected={selection.has(thread.id)}
        focusable={thread.id === rovingId}
        dragging={dragged}
        highlighted={highlighted}
        nested={nested}
        showProject={showProject}
        ledger={ledger}
        verb={verb}
        actions={actions}
      />
    );
  };

  const dragTarget = drag?.target?.section ?? null;
  const dragging = drag !== null;
  const noActiveRows = shown.active.length === 0 && emptyProjects.length === 0;
  // The Working shelf sits at the top of the list in every view, so a running thread
  // never moves a shelf to the bottom and leaves the Projects header with nothing under it.
  const shelfAbove = shelves.length > 0;
  const workingGroup = (
    <Group id="working" label="Working" count={shelves.length} defaultOpen>
      {shelves.map((thread) => row(thread, "working", false))}
    </Group>
  );
  const hasShelf =
    shown.snoozed.length > 0 || shown.settled.length > 0 || dragging;

  const setGroupingOverride = (projectId: string, mode: ProjectGroupingMode | null) => {
    const next = { ...projectGroupingOverrides };
    if (mode === null) delete next[projectId];
    else next[projectId] = mode;
    setPrefs({ projectGroupingOverrides: next });
  };

  const folderMenu = (event: React.MouseEvent, folder: ProjectFolder) => {
    setMenu({
      x: event.clientX,
      y: event.clientY,
      label: `Actions for ${folder.label}`,
      items: folderMenuItems(folder, { projectGroupingOverrides, projectGrouping, setGroupingOverride }),
    });
  };

  return (
    <nav ref={nav} className="sidebar" aria-label="Workspace">
      <div className="sidebar-search">
        <button type="button" className="search-trigger" onClick={() => setPalette("root")}>
          <Search size={14} aria-hidden />
          <span>Search or ask</span>
          <kbd>{/mac/i.test(navigator.platform) ? "⌘K" : "Ctrl K"}</kbd>
        </button>
      </div>
      {ledger ? null : <SidebarNav route={route} ledger={false} />}
      <div className="side-label">
        <span>{ledger ? "Threads" : "Projects"}</span>
        <span className="side-label-tools">
          <IconButton
            label={searching ? "Close thread search" : "Search threads"}
            pressed={searching}
            onClick={() => {
              if (searching) setQuery("");
              setSearching(!searching);
            }}
          >
            <Search size={14} aria-hidden />
          </IconButton>
          <ProjectFilter projects={list} />
          <IconButton label="Add project" onClick={() => setPalette("add-project")}>
            <Plus size={14} aria-hidden />
          </IconButton>
        </span>
      </div>
      {searching ? (
        <SearchField
          query={query}
          highlight={highlight}
          results={results}
          setQuery={setQuery}
          setHighlight={setHighlight}
          setSearching={setSearching}
        />
      ) : null}
      <div className="sidebar-scroll" ref={tree} onKeyDown={onTreeKey} data-dragging={dragging || undefined}>
        {projects.error && !projects.data ? (
          <ErrorState compact title="Couldn't load projects" failure={projects.error} onRetry={() => void refreshAll()} />
        ) : threadError && !threadsLoaded ? (
          <ErrorState compact title="Couldn't load threads" failure={threadError} onRetry={() => void refreshAll()} />
        ) : !projects.data || (list.length > 0 && !threadsLoaded) ? (
          <Loading label="Loading threads" rows={5} />
        ) : isSearching ? (
          results.length === 0 ? (
            <p className="sidebar-empty">No matching threads</p>
          ) : (
            <ul className="thread-list" role="list" aria-label="Thread search results">
              {results.map((thread, at) =>
                row(thread, sectionOf(thread, Date.now()) ?? "active", false, at === highlight),
              )}
            </ul>
          )
        ) : (
          <>
            {shelfAbove ? <div className="sidebar-groups sidebar-shelf">{workingGroup}</div> : null}
            <ul className="thread-list">
              <DraftRow route={route} />
              {dragging ? <DragBoundary label="Pinned" section="pinned" active={dragTarget === "pinned"} /> : null}
              {shown.pinned.map((thread) => row(thread, "pinned", false))}
              {dragging ? <DragBoundary label="Active" section="active" active={dragTarget === "active"} /> : null}
              {ledger
                ? ledgerGroups.map((group) =>
                    group.rows.length === 0
                      ? null
                      : [
                          <li key={`ledger:${group.key}`} className="ledger-head" data-tone={group.tone}>
                            <span>{group.label}</span>
                            <span className="group-count">{group.rows.length}</span>
                          </li>,
                          ...group.rows.map((thread) => row(thread, "active", false)),
                        ],
                  )
                : foldersView
                ? folders.map((folder) => {
                    const key = `folder:${folder.key}`;
                    const collapsed = closedGroups[key] === false;
                    // A project with no threads keeps its own row: identity, trust state and a way in.
                    const bare = folder.projects.filter((project) => emptyProjects.some((empty) => empty.id === project.id));
                    if (folder.threads.length === 0 && bare.length > 0) {
                      return bare.map((project) => <EmptyProjectRow key={project.id} project={project} />);
                    }
                    return [
                      <FolderHeader
                        key={key}
                        project={folder.project}
                        label={folder.label}
                        collapsed={collapsed}
                        count={folder.threads.length}
                        needs={folder.threads.filter((thread) => attentionRank(thread) === 0).length}
                        onToggle={() => toggleGroup(key, true)}
                        onMenu={(event) => folderMenu(event, folder)}
                      />,
                      ...(collapsed ? [] : folder.threads.map((thread) => row(thread, "active", true, false, true))),
                    ];
                  })
                : shown.active.map((thread) => row(thread, "active", false))}
              {strays.map((thread) => row(thread, "active", false, false, false, true))}
              {foldersView ? null : emptyProjects.map((project) => <EmptyProjectRow key={project.id} project={project} />)}
              {dragging && noActiveRows ? (
                <DropPlaceholder label="Active" section="active" active={dragTarget === "active"} />
              ) : null}
            </ul>
            {hasShelf ? (
              <div className="sidebar-groups">
                {shown.snoozed.length > 0 ? (
                  <Group id="snoozed" label="Snoozed" count={shown.snoozed.length} defaultOpen>
                    {shown.snoozed.map((thread) => row(thread, "snoozed", true))}
                  </Group>
                ) : null}
                {shown.settled.length > 0 || dragging ? (
                  <Group
                    id="settled"
                    label="Settled"
                    count={shown.settled.length}
                    defaultOpen={false}
                    dropSection="settled"
                    forceOpen={dragging}
                  >
                    {settledRows.map((thread) => row(thread, "settled", true))}
                    {shown.settled.length === 0 ? (
                      <DropPlaceholder label="Settled" section="settled" active={dragTarget === "settled"} />
                    ) : null}
                    {shown.settled.length > settledRows.length ? (
                      <li>
                        <button
                          type="button"
                          className="show-more"
                          onClick={() => setSettledVisible(settledVisible + SETTLED_PAGE)}
                        >
                          Show {Math.min(SETTLED_PAGE, shown.settled.length - settledRows.length)} more
                        </button>
                      </li>
                    ) : null}
                  </Group>
                ) : null}
              </div>
            ) : null}
          </>
        )}
      </div>
      <SidebarFoot route={route} ledger={ledger} />
      <SidebarRail />
      <Presence>{menu ? <Menu {...menu} onClose={() => setMenu(null)} /> : null}</Presence>
      {renamingThread ? <RenameThreadDialog thread={renamingThread} onClose={() => setRenaming(null)} /> : null}
      {snoozing ? <CustomSnoozeDialog threads={snoozing} onClose={() => setSnoozing(null)} /> : null}
      {deleting ? (
        <DeleteThreadsDialog threads={deleting} onClose={() => setDeletingState(null)} />
      ) : null}
    </nav>
  );
});
