import { Minus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { api } from "../../../api/client";
import { useAsync } from "../../../lib/useAsync";
import { registerRefresh, under, EVERYTHING } from "../../../lib/dataRefresh";
import { useNow } from "../../../lib/useNow";
import { chipId, languageForPath } from "../../../product/composer/composerContext";
import { primaryEditor } from "../../../product/composer/editorPreferences";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { navigate, type Route } from "../../../product/state/route";
import { fileTabId, useUi, type FileTarget } from "../../../product/state/ui";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { copyText } from "../../../product/ui/copyText";
import { Loading } from "../../../product/ui/states";
import { IconButton } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { arrangeThreads, attentionRank } from "../sidebar/groups";
import { ExplorerRow, type RowHandlers } from "./ExplorerRow";
import { LeftBarSwitch } from "./LeftBarSwitch";
import { buildRows, samePath, worktreeLabel, type Row } from "./model";
import { startThreadIn } from "./startThread";
import { levelKey, useExplorer } from "./store";

/** A row's height with the 1px gap under it; rows of a long list are placed by it. */
const PITCH = 25;
/** A list this long draws only the rows on screen and a few around them. */
const VIRTUAL_FROM = 150;
const OVERSCAN = 8;

interface MenuState {
  x: number;
  y: number;
  label: string;
  items: MenuItem[];
  row: Row;
}

/** The explorer: every project, its threads and its worktrees, and the files of any worktree unfolded. */
export function Explorer({ route }: { route: Route }) {
  const visible = useVisibleProjects();
  const projects = useMemo(() => visible ?? [], [visible]);
  const projectsState = useWorkspace((state) => state.projects);
  const threads = useWorkspace((state) => state.threads);
  const explorerOpen = useUi((state) => state.explorerOpen);
  const setPrefs = useUi((state) => state.setPrefs);
  const activeTab = useUi((state) => (state.panelOpen ? state.panelTab : null));
  const preferred = useUi((state) => state.preferredEditor);
  const worktrees = useExplorer((state) => state.worktrees);
  const levels = useExplorer((state) => state.levels);
  const shown = useExplorer((state) => state.shown);

  const currentThreadId = route.name === "thread" ? route.id : null;
  const current = currentThreadId ? threads[currentThreadId] : undefined;
  const currentProjectId = current?.project_id ?? (route.name === "project" || route.name === "new" ? route.projectId : null);

  const now = useNow(true, 30_000);
  const { byProject, needs } = useMemo(() => {
    const sections = arrangeThreads(Object.values(threads), now, false, false);
    const byProject: Record<string, typeof sections.active> = {};
    const needs: Record<string, number> = {};
    for (const thread of [...sections.pinned, ...sections.active]) {
      (byProject[thread.project_id] ??= []).push(thread);
      if (attentionRank(thread) === 0) needs[thread.project_id] = (needs[thread.project_id] ?? 0) + 1;
    }
    return { byProject, needs };
  }, [threads, now]);

  const rows = useMemo(
    () =>
      buildRows({ projects, threads: byProject, currentThreadId, currentProjectId, worktrees, levels, shown, explorerOpen, needs }),
    [projects, byProject, currentThreadId, currentProjectId, worktrees, levels, shown, explorerOpen, needs],
  );

  // A row that waits for its data asks for it: a folder is read when it is unfolded, not before.
  useEffect(() => {
    const store = useExplorer.getState();
    for (const row of rows) {
      if (row.kind !== "loading") continue;
      if (row.level) void store.loadLevel(row.projectId, row.level.worktreeId, row.level.path);
      else void store.loadWorktrees(row.projectId, { thread: currentThreadId });
    }
  }, [rows, currentThreadId]);

  // What was read earlier is shown at once, then read again; the service says when a project's files changed.
  const latest = useRef(currentThreadId);
  latest.current = currentThreadId;
  useEffect(() => {
    const again = (projectIds: string[]) => {
      const store = useExplorer.getState();
      for (const id of projectIds) void store.reload(id, { thread: latest.current, quiet: true });
    };
    const loaded = () => Object.keys(useExplorer.getState().worktrees);
    again(loaded());
    return registerRefresh(under("explorer"), (heard) => {
      const named = heard.some((what) => what === EVERYTHING) ? loaded() : heard.map((what) => what.slice("explorer:".length));
      again([...new Set(named)].filter((id) => id in useExplorer.getState().worktrees));
    });
  }, []);
  // Which worktrees are read-only for a thread depends on the thread asking.
  useEffect(() => {
    const store = useExplorer.getState();
    for (const id of Object.keys(store.worktrees)) void store.loadWorktrees(id, { thread: currentThreadId, quiet: true });
  }, [currentThreadId]);

  // The long list draws only what is on screen.
  const scroller = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ top: 0, height: 0 });
  const measure = useCallback(() => {
    const element = scroller.current;
    if (element) setView({ top: element.scrollTop, height: element.clientHeight });
  }, []);
  useLayoutEffect(() => {
    measure();
    const element = scroller.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [measure]);
  const windowed = view.height > 0 && rows.length > VIRTUAL_FROM;
  const start = windowed ? Math.max(0, Math.floor(view.top / PITCH) - OVERSCAN) : 0;
  const end = windowed ? Math.min(rows.length, Math.ceil((view.top + view.height) / PITCH) + OVERSCAN) : rows.length;
  const drawn = rows.slice(start, end);

  // The tree is one tab stop: the row last focused, else the open thread, else the first.
  const navigable = useCallback((row: Row) => row.kind !== "loading" && row.kind !== "empty" && row.kind !== "error", []);
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const wantFocus = useRef(false);
  const stop =
    drawn.find((row) => row.key === focusKey && navigable(row)) ??
    drawn.find((row) => row.kind === "thread" && row.thread?.id === currentThreadId) ??
    drawn.find(navigable);
  const focusRow = (row: Row | undefined) => {
    if (!row) return;
    wantFocus.current = true;
    setFocusKey(row.key);
    const at = rows.indexOf(row);
    const element = scroller.current;
    if (windowed && element && (at * PITCH < element.scrollTop || (at + 1) * PITCH > element.scrollTop + element.clientHeight)) {
      element.scrollTop = Math.max(0, at * PITCH - element.clientHeight / 2);
      measure();
    }
  };
  useLayoutEffect(() => {
    if (!wantFocus.current) return;
    const element = scroller.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(focusKey ?? "")}"]`);
    if (!element) return;
    wantFocus.current = false;
    element.focus({ preventScroll: false });
  });

  const editors = useAsync("editors", (signal) => api.editors(signal));
  const [menu, setMenu] = useState<MenuState | null>(null);

  const toggle = (row: Row) => setPrefs({ explorerOpen: { ...explorerOpen, [row.key]: !row.open } });

  /** The target a file row opens as a tab. */
  const targetOf = (row: Row): FileTarget | null => {
    const project = projects.find((one) => one.id === row.projectId);
    if (!row.worktree || row.path === undefined || !project) return null;
    return { kind: "explorer", projectId: row.projectId, worktreeId: row.worktree.id, path: row.path, project: project.name, worktree: worktreeLabel(row.worktree) };
  };
  const openFile = (row: Row) => {
    const target = targetOf(row);
    if (target) useUi.getState().openFileTab(target);
  };
  /** The file is in the worktree the open thread runs in, so the thread's own actions apply to it. */
  const ownsFile = (row: Row) => !!(current && row.worktree && samePath(row.worktree.path, current.cwd));
  const outsideEditor = (row: Row) => {
    const editor = primaryEditor(editors.data ?? [], preferred);
    if (!editor || !current || row.path === undefined) return;
    api.openInEditor(current.id, { path: row.path, editor: editor.id }).catch((failure: unknown) => {
      useUi.getState().pushToast(`Couldn't open it. ${failure instanceof Error ? failure.message : ""}`.trim());
    });
  };
  const addToThread = (row: Row) => {
    if (!current || row.path === undefined) return;
    useComposerChips.getState().add(current.id, { id: chipId(), kind: "file", path: row.path, line: null, excerpt: null, language: languageForPath(row.path) });
    useUi.getState().pushToast("File added to your message");
  };

  const openMenu = (row: Row, x: number, y: number) => {
    if (row.kind === "worktree" && row.worktree) {
      const worktree = row.worktree;
      setMenu({
        x,
        y,
        label: "Worktree actions",
        row,
        items: [
          { label: "New thread here", onSelect: () => startThreadIn(row.projectId, worktree) },
          { label: "Copy path", onSelect: () => void copyText(worktree.path, "Path copied") },
        ],
      });
      return;
    }
    if (row.kind !== "file" || !row.worktree || row.path === undefined) return;
    const { worktree, path } = row;
    const owned = ownsFile(row);
    const editor = owned ? primaryEditor(editors.data ?? [], preferred) : null;
    const manager = owned ? (editors.data ?? []).find((one) => one.file_manager) : undefined;
    const separator = worktree.path.includes("\\") ? "\\" : "/";
    setMenu({
      x,
      y,
      label: "File actions",
      row,
      items: [
        { label: "Open", shortcut: "↵", onSelect: () => openFile(row) },
        ...(editor ? [{ label: "Open in an outside editor", shortcut: "⌥↵", onSelect: () => outsideEditor(row) }] : []),
        ...(owned ? [{ label: "Add to this thread", shortcut: "@", onSelect: () => addToThread(row) }] : []),
        { label: "Copy path", separatorBefore: true, onSelect: () => void copyText(`${worktree.path.replace(/[\\/]+$/, "")}${separator}${path.split("/").join(separator)}`, "Path copied") },
        { label: "Copy relative path", onSelect: () => void copyText(path, "Path copied") },
        ...(manager && current
          ? [{ label: `Reveal in ${manager.name}`, onSelect: () => void api.openInEditor(current.id, { path, editor: manager.id }).catch(() => undefined) }]
          : []),
      ],
    });
  };

  const activate = (row: Row, event: MouseEvent | KeyboardEvent) => {
    setFocusKey(row.key);
    if (row.kind === "file") return openFile(row);
    if (row.kind === "thread" && row.thread) return navigate({ name: "thread", id: row.thread.id });
    if (row.kind === "more" && row.level) return useExplorer.getState().showMore(levelKey(row.projectId, row.level.worktreeId, row.level.path));
    if (row.kind === "error") {
      const store = useExplorer.getState();
      if (row.retry === "level" && row.level) void store.loadLevel(row.projectId, row.level.worktreeId, row.level.path);
      else void store.loadWorktrees(row.projectId, { thread: currentThreadId });
      return;
    }
    if (event.type === "click" || event.type === "keydown") toggle(row);
  };
  const handlers: RowHandlers = { onActivate: activate, onMenu: openMenu, onFocus: (row) => setFocusKey(row.key) };

  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-key]");
    const nav = rows.filter(navigable);
    const row = rows.find((one) => one.key === element?.dataset.key);
    if (!row || (event.target as HTMLElement).closest("button")) return;
    const at = nav.indexOf(row);
    const expandable = row.kind === "project" || row.kind === "group" || row.kind === "worktree" || row.kind === "dir";
    const move = (to: Row | undefined) => {
      event.preventDefault();
      focusRow(to);
    };
    switch (event.key) {
      case "ArrowDown":
        return move(nav[at + 1]);
      case "ArrowUp":
        return move(nav[at - 1]);
      case "Home":
        return move(nav[0]);
      case "End":
        return move(nav.at(-1));
      case "ArrowRight":
        if (!expandable) return;
        event.preventDefault();
        if (!row.open) toggle(row);
        else if (nav[at + 1] && nav[at + 1]!.depth > row.depth) focusRow(nav[at + 1]);
        return;
      case "ArrowLeft": {
        event.preventDefault();
        if (expandable && row.open) return toggle(row);
        const parent = [...nav.slice(0, at)].reverse().find((one) => one.depth < row.depth);
        return focusRow(parent);
      }
      case "Enter":
        event.preventDefault();
        if (event.altKey && row.kind === "file") return outsideEditor(row);
        if (row.kind === "file") {
          openFile(row);
          // Enter takes the person to the tab it opened; Space leaves focus here.
          requestAnimationFrame(() => document.querySelector<HTMLElement>('.ctx-tabs [role="tab"][aria-selected="true"]')?.focus());
          return;
        }
        return activate(row, event);
      case " ":
        event.preventDefault();
        return row.kind === "file" ? openFile(row) : activate(row, event);
      case "@":
        if (row.kind === "file" && ownsFile(row)) {
          event.preventDefault();
          addToThread(row);
        }
        return;
      case "ContextMenu":
        event.preventDefault();
        return openMenu(row, element!.getBoundingClientRect().left, element!.getBoundingClientRect().bottom);
      case "F10":
        if (!event.shiftKey) return;
        event.preventDefault();
        return openMenu(row, element!.getBoundingClientRect().left, element!.getBoundingClientRect().bottom);
      default:
    }
  };

  const collapseAll = () => {
    const closed = Object.fromEntries(rows.filter((row) => row.open).map((row) => [row.key, false]));
    setPrefs({ explorerOpen: { ...explorerOpen, ...closed } });
  };
  const refresh = () => {
    const store = useExplorer.getState();
    for (const id of Object.keys(store.worktrees)) void store.reload(id, { thread: currentThreadId, refresh: true });
  };

  const selectedTab = activeTab;
  const isSelected = (row: Row) => {
    if (row.kind === "thread") return row.thread?.id === currentThreadId;
    if (row.kind !== "file") return false;
    const target = targetOf(row);
    return target !== null && selectedTab === fileTabId(target);
  };

  return (
    <>
      <div className="side-label">
        <LeftBarSwitch />
        <span className="side-label-tools">
          <IconButton label="Collapse all" onClick={collapseAll}>
            <Minus size={14} aria-hidden />
          </IconButton>
          <IconButton label="Refresh" onClick={refresh}>
            <RefreshCw size={14} aria-hidden />
          </IconButton>
        </span>
      </div>
      <div className="sidebar-scroll x2scroll" ref={scroller} onScroll={measure}>
        {!projectsState.data ? (
          <Loading label="Loading projects" rows={5} />
        ) : (
          <div className="x2tree" role="tree" aria-label="Projects, worktrees and files" onKeyDown={onKey}>
            {start > 0 ? <div style={{ height: start * PITCH }} aria-hidden /> : null}
            {drawn.map((row) => (
              <ExplorerRow
                key={row.key}
                row={row}
                selected={isSelected(row)}
                focusable={row.key === stop?.key}
                menuOpen={menu?.row.key === row.key}
                draggable={row.kind === "file" && ownsFile(row)}
                handlers={handlers}
              />
            ))}
            {end < rows.length ? <div style={{ height: (rows.length - end) * PITCH }} aria-hidden /> : null}
          </div>
        )}
      </div>
      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            label={menu.label}
            items={menu.items}
            onClose={() => setMenu(null)}
          />
        ) : null}
      </Presence>
    </>
  );
}
