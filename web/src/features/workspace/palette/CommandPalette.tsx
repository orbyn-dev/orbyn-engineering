import { File, FolderPlus, GitPullRequest, MessageSquare, RotateCcw, Search, Settings, Sparkles, SquarePen, SunMoon, TextSearch, TriangleAlert } from "lucide-react";
import { useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { api, pullsApi } from "../api";
import { compactAge } from "../../../lib/format";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { navigate, parseRoute, SETTINGS_SECTIONS, type Route } from "../../../product/state/route";
import { useUi, type PaletteStart } from "../../../product/state/ui";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { ProjectBadge } from "../../../product/ui/badges";
import { useAccountAvailable } from "../../account";
import { Button } from "../../../ui/controls";
import { AddProjectPalette } from "./AddProjectPalette";
import { loadFrecency, rankEntries, recordUse, saveFrecency } from "./paletteRanking";

interface Entry {
  key: string;
  group: string;
  label: string;
  meta?: string;
  icon: ReactNode;
  /** Extra words that find this entry, such as a thread's project and branch. */
  search?: string;
  /** Picking this row counts toward its frecency (threads and pull requests only). */
  track?: boolean;
  route:
    | Route
    | "add-project"
    | "new-in"
    | "toggle-theme"
    | "restart-session"
    | { file: string; line: number | null }
    | { message: string; thread: string };
}

const title = (section: string) =>
  section.charAt(0).toUpperCase() + section.slice(1);

const searchTitle: Partial<Record<PaletteStart, string>> = {
  files: "Go to file",
  search: "Search in project",
};

/** Files or text in the open thread's checkout. */
function useCheckoutSearch(start: PaletteStart, query: string) {
  const route = parseRoute(window.location.hash);
  const threadId = route.name === "thread" ? route.id : null;
  const content = start === "search";
  const key = `palette:${start}:${threadId ?? ""}:${query}`;
  const hits = useAsync(key, async (signal) => {
    if (!threadId || !query.trim() || !searchTitle[start]) return [];
    return api.searchFiles(threadId, query.trim(), content, signal);
  });
  return hits.data ?? [];
}

const MESSAGE_SEARCH_MIN = 2;
const MESSAGE_SEARCH_DELAY_MS = 150;

/** Whole messages across threads. The service decides when a query is long enough. */
function useMessageSearch(query: string, enabled: boolean) {
  const needle = query.trim();
  const hits = useAsync(`palette:messages:${enabled ? needle : ""}`, async (signal) => {
    if (!enabled || needle.length < MESSAGE_SEARCH_MIN) return [];
    await new Promise((done) => setTimeout(done, MESSAGE_SEARCH_DELAY_MS));
    if (signal.aborted) return [];
    return (await api.searchMessages(needle, signal)).hits;
  });
  return hits.data ?? [];
}

type PullList = Awaited<ReturnType<typeof pullsApi.list>>;
type PullEntry = PullList["pulls"][number];

/**
 * Open pull requests across projects, read from the list the Pull requests page uses, only once a query is typed.
 * A project whose forge could not be read is named by the service in the list's own `failed` rows rather than by
 * failing the whole answer, so those rows are what mark the group unreadable. Retry is the person's own re-read: the
 * service keeps a list for a while, so it asks for a fresh one instead of the answer it already served.
 */
function usePullSearch(query: string, enabled: boolean): { pulls: PullEntry[]; status: "idle" | "loading" | "error"; retry(): void } {
  const on = enabled && query.trim().length >= MESSAGE_SEARCH_MIN;
  const list = useAsync<PullList>(`palette:pulls:${on ? "on" : "off"}`, async (signal, how) => {
    if (!on) return { pulls: [], failed: [] };
    return pullsApi.list("open", "", signal, undefined, how.force);
  });
  const unreadable = list.status === "error" || (list.data?.failed ?? []).length > 0;
  return {
    pulls: list.data?.pulls ?? [],
    status: !on ? "idle" : list.status === "loading" ? "loading" : unreadable ? "error" : "idle",
    retry: list.reloadNow,
  };
}

/**
 * What the palette says in the Pull requests group while the list is read or cannot be (palette-prs-loading,
 * palette-prs-unreadable). A partial read can have rows under the group's heading already, so `heading` is false then:
 * the note sits under the one heading those rows drew.
 */
function PullsNote({ status, heading, onRetry }: { status: "loading" | "error"; heading: boolean; onRetry(): void }) {
  return (
    <div>
      {heading ? <div className="palette-group">Pull requests</div> : null}
      <div className="palette-note" role="status">
        {status === "error" ? <TriangleAlert className="palette-note-icon" size={14} aria-hidden /> : null}
        <span>{status === "loading" ? "Looking for pull requests…" : "Couldn’t read pull requests. Threads and commands still work."}</span>
        {status === "error" ? <Button small onClick={onRetry}>Retry</Button> : null}
      </div>
    </div>
  );
}

export function CommandPalette({ start, onClose }: { start: PaletteStart; onClose(): void }) {
  const threads = useWorkspace((state) => state.threads);
  const allProjects = useWorkspace((state) => state.projects.data);
  const [adding, setAdding] = useState(start === "add-project" || start === "add-project-url" || start === "add-project-new");
  const [subdialog, setSubdialog] = useState(false);
  const ref = useRef<HTMLDialogElement>(null);
  const [newIn, setNewIn] = useState(false);
  const projects = useVisibleProjects();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.showModal();
    return () => {
      if (dialog.open) dialog.close();
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  const hits = useCheckoutSearch(start, query);
  const messages = useMessageSearch(query, !searchTitle[start] && !newIn);
  const accountOn = useAccountAvailable();
  const { pulls, status: pullStatus, retry: retryPulls } = usePullSearch(query, !searchTitle[start] && !newIn);
  const [book, setBook] = useState(() => loadFrecency());
  const entries = useMemo<Entry[]>(() => {
    if (searchTitle[start]) {
      return hits.map((hit) => ({
        key: `${hit.path}:${hit.line}`,
        group: searchTitle[start] ?? "",
        label: hit.text && start === "search" ? hit.text.trim() : hit.path,
        meta: start === "search" ? `${hit.path}:${hit.line}` : undefined,
        icon: <File size={16} aria-hidden />,
        route: { file: hit.path, line: hit.line || null },
      }));
    }
    const actions: Entry[] = [
      { key: "theme", group: "Actions", label: "Toggle theme", icon: <SunMoon size={16} aria-hidden />, route: "toggle-theme" },
      { key: "new", group: "Actions", label: "New thread", icon: <SquarePen size={16} aria-hidden />, route: { name: "new", projectId: null } },
      { key: "new-in", group: "Actions", label: "New thread in...", icon: <SquarePen size={16} aria-hidden />, route: "new-in" },
      { key: "welcome", group: "Actions", label: "Welcome", icon: <Sparkles size={16} aria-hidden />, route: { name: "welcome" } as Route },
      { key: "add", group: "Actions", label: "Add project", icon: <FolderPlus size={16} aria-hidden />, route: "add-project" },
      ...(parseRoute(window.location.hash).name === "thread"
        ? [
            { key: "restart", group: "Actions", label: "Restart agent session", icon: <RotateCcw size={16} aria-hidden />, route: "restart-session" as const },
          ]
        : []),
      ...SETTINGS_SECTIONS.filter((section) => section !== "account" || accountOn).map((section) => ({
        key: `settings:${section}`,
        group: "Settings",
        label: title(section),
        // The group's own word finds these, so "sett" and "settings" list the Settings pages.
        search: "settings preferences",
        icon: <Settings size={16} aria-hidden />,
        route: { name: "settings", section } as Route,
      })),
    ];
    const recent = Object.values(threads)
      .filter((thread) => !thread.lifecycle?.archived_at)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .map((thread) => ({
        key: thread.id,
        track: true,
        group: "Threads",
        label: thread.title,
        meta: compactAge(thread.updated_at),
        search: `${allProjects?.find((project) => project.id === thread.project_id)?.name ?? ""} ${thread.worktree?.branch ?? ""}`,
        icon: <MessageSquare size={16} aria-hidden />,
        route: { name: "thread", id: thread.id } as Route,
      }));
    const needle = query.trim().toLowerCase();
    const inProjects: Entry[] = (projects ?? []).map((project) => ({
      key: `in:${project.id}`,
      group: "Projects",
      label: project.name,
      icon: <ProjectBadge name={project.name} project={project} />,
      route: { name: "new", projectId: project.id },
    }));
    const all = newIn ? inProjects : [...actions, ...recent];
    const words = needle.split(/\s+/).filter(Boolean);
    const found = (entry: Entry) => {
      const text = `${entry.label} ${entry.search ?? ""}`.toLowerCase();
      return words.every((word) => text.includes(word));
    };
    const inMessages: Entry[] = messages.map((hit) => ({
      key: `message:${hit.thread_id}:${hit.message_id}:${hit.seq}`,
      group: "Messages",
      label: hit.snippet,
      meta: hit.thread_title,
      icon: <TextSearch size={16} aria-hidden />,
      route: { message: hit.message_id, thread: hit.thread_id },
    }));
    const inPulls: Entry[] = newIn
      ? []
      : pulls.map(({ project_id, pull }) => ({
          key: `pull:${project_id}:${pull.number}`,
          track: true,
          group: "Pull requests",
          label: pull.title,
          meta: `#${pull.number}`,
          search: `${allProjects?.find((project) => project.id === project_id)?.name ?? ""} ${pull.head}`,
          icon: <GitPullRequest size={16} aria-hidden />,
          route: { name: "pulls", project: project_id, number: Number(pull.number) } as Route,
        }));
    const matching = needle ? [...all, ...inPulls].filter(found) : all;
    return [...rankEntries(matching, query, book, Date.now()).slice(0, 50), ...inMessages];
  }, [threads, query, newIn, projects, allProjects, hits, start, messages, accountOn, pulls, book]);

  const choose = (entry: Entry | undefined) => {
    if (!entry) return;
    // Only a person's pick is remembered; opening the palette or listing rows writes nothing.
    if (entry.track) {
      const next = recordUse(book, entry.key, Date.now());
      setBook(next);
      saveFrecency(next);
    }
    if (entry.route === "add-project") {
      setAdding(true);
      return;
    }
    if (entry.route === "restart-session") {
      onClose();
      const opened = parseRoute(window.location.hash);
      if (opened.name !== "thread") return;
      const { pushToast } = useUi.getState();
      api.restartSession(opened.id).then(
        () => pushToast("The next message starts a fresh agent session"),
        (failure: unknown) => pushToast(`Couldn't restart. ${toFailure(failure).message}`),
      );
      return;
    }
    if (entry.route === "toggle-theme") {
      const { theme, setTheme } = useUi.getState();
      setTheme(theme === "dark" ? "light" : "dark");
      onClose();
      return;
    }
    if (typeof entry.route === "object" && "file" in entry.route) {
      const route = parseRoute(window.location.hash);
      const thread = route.name === "thread" ? useWorkspace.getState().threads[route.id] : null;
      if (thread) useUi.getState().revealFile(entry.route.file, entry.route.line, { threadId: thread.id, cwd: thread.cwd });
      onClose();
      return;
    }
    if (typeof entry.route === "object" && "message" in entry.route) {
      useUi.getState().revealMessage(entry.route.thread, entry.route.message);
      onClose();
      navigate({ name: "thread", id: entry.route.thread });
      return;
    }
    if (entry.route === "new-in") {
      setNewIn(true);
      setQuery("");
      setActive(0);
      return;
    }
    onClose();
    navigate(entry.route);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((active + step + entries.length) % Math.max(entries.length, 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(entries[Math.min(active, entries.length - 1)]);
    }
  };

  let group = "";
  const noteAt = entries.some((entry) => entry.group === "Messages") ? entries.findIndex((entry) => entry.group === "Messages") : entries.length;
  return (
    <dialog
      ref={ref}
      className="palette"
      data-subdialog={subdialog || undefined}
      aria-label="Command palette"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
    >
      {adding ? (
        <AddProjectPalette
          initialView={start === "add-project-url" ? "clone" : start === "add-project-new" ? "new" : "folders"}
          onClose={onClose}
          onSubdialog={setSubdialog}
        />
      ) : (
        <>
      <div className="palette-search">
        <Search size={16} aria-hidden />
        <input
          autoFocus
          role="combobox"
          aria-expanded
          aria-controls="palette-list"
          aria-label="Search"
          placeholder={searchTitle[start] ?? "Search threads, pull requests and commands"}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "Backspace" && newIn && query === "") setNewIn(false);
            onKeyDown(event);
          }}
        />
      </div>
      <div id="palette-list" className="palette-list">
        {entries.length === 0 && pullStatus === "idle" ? <p className="sidebar-empty">No results</p> : null}
        {entries.map((entry, at) => {
          // The pull request line sits after the ranked groups, ahead of the messages.
          // The note keeps the heading only when the rows above it did not draw the group's own.
          const note = pullStatus !== "idle" && at === noteAt ? <PullsNote key="pulls-note" status={pullStatus} heading={group !== "Pull requests"} onRetry={retryPulls} /> : null;
          const heading = entry.group !== group ? entry.group : null;
          group = entry.group;
          return (
            <div key={entry.key}>
              {note}
              {heading ? <div className="palette-group">{heading}</div> : null}
              <button
                type="button"
                data-active={at === Math.min(active, entries.length - 1) || undefined}
                onMouseMove={() => setActive(at)}
                onClick={() => choose(entry)}
              >
                {entry.icon}
                <span className="palette-title">{entry.label}</span>
                {entry.meta ? <span>{entry.meta}</span> : null}
              </button>
            </div>
          );
        })}
        {pullStatus !== "idle" && noteAt >= entries.length ? <PullsNote status={pullStatus} heading={group !== "Pull requests"} onRetry={retryPulls} /> : null}
      </div>
      <footer className="palette-foot">
        <span>
          <kbd>↑</kbd>
          <kbd>↓</kbd> Navigate
        </span>
        <span>
          <kbd>Enter</kbd> Select
        </span>
        <span>
          <kbd>Esc</kbd> Close
        </span>
      </footer>
        </>
      )}
    </dialog>
  );
}
