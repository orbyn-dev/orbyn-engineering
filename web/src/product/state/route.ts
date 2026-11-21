import { useSyncExternalStore } from "react";
import { deferNavigation, navigationSnapshot } from "./navigationGuard";
import type { ProjectSettingsSelection } from "../../generated/ProjectSettingsSelection";

export const SETTINGS_SECTIONS = [
  "general",
  "account",
  "appearance",
  "chat",
  "diffs",
  "keybindings",
  "providers",
  "tools",
  "threads",
  "tasks",
  "projects",
  "source-control",
  "worktrees",
  "crew",
  "secrets",
  "maintenance",
  "labs",
  "about",
  "archive",
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

/** The Crew screens built so far; the rest join as they land. */
export const CREW_TABS = ["approvals", "trust", "background", "inbox", "flow", "board", "map", "lanes"] as const;
export type CrewTab = (typeof CREW_TABS)[number];

export const CREW_SETTINGS_PAGES = ["areas", "thresholds", "capacity", "coverage", "escapes", "possible", "escape-settings", "escape", "backtest", "estimate", "report"] as const;
export type CrewSettingsPage = (typeof CREW_SETTINGS_PAGES)[number];

/** Where the coverage map was opened from. It comes from the address the link carried, never from history: no origin, no way back drawn. */
export const CREW_ORIGINS = ["capacity", "escapes"] as const;
export type CrewOrigin = (typeof CREW_ORIGINS)[number];

export type Route =
  | { name: "home" }
  | { name: "thread"; id: string }
  | { name: "new"; projectId: string | null }
  /** A project with no threads: the empty screen, or the composer once it has some. */
  | { name: "project"; projectId: string }
  | { name: "trust"; projectId: string; threadId: string | null }
  | {
      name: "settings";
      section: SettingsSection | null;
      selection?: ProjectSettingsSelection;
      machine?: string;
      task?: string;
      /** With `task`: open the editor instead of the task page. */
      edit?: boolean;
      /**
       * Settings › Crew › Autopilot by area (and its thresholds), for the repository `repo` (the first one when absent), or a
       * page that picks its own project: Review capacity and its coverage map (`repo` names the project the coverage map
       * opens for), the escape log and one escape (`id`), the reviewer backtest, its estimate and one run's report (`id`).
       */
      crewPage?: CrewSettingsPage;
      repo?: string;
      /** With the coverage page: the page whose link opened it, so it can offer the way back. */
      from?: CrewOrigin;
      /** The escape or backtest run an `escape` or `report` page names. */
      id?: string;
    }
  | { name: "usage" }
  | { name: "skills"; skill?: string }
  | { name: "tasks" }
  /** Crew Preview: drawn only while the service says Crew is on. */
  | { name: "crew"; tab: CrewTab; task?: string }
  /** The list, or one pull request of one project. */
  | { name: "pulls"; project?: string; number?: number };

const enc = encodeURIComponent;

/** Where each Settings › Crew page lives under `#/settings/crew/`. */
const CREW_PAGE_PATH: Record<CrewSettingsPage, string> = {
  areas: "areas",
  thresholds: "areas/thresholds",
  capacity: "capacity",
  coverage: "capacity/coverage",
  escapes: "escapes",
  possible: "escapes/possible",
  "escape-settings": "escapes/settings",
  escape: "escapes/detail",
  backtest: "backtest",
  estimate: "backtest/estimate",
  report: "backtest/report",
};
const CREW_PAGES_WITH_REPO: readonly CrewSettingsPage[] = ["areas", "thresholds", "capacity", "coverage", "escapes", "possible"];
const CREW_PAGES_WITH_ORIGIN: readonly CrewSettingsPage[] = ["coverage"];
const CREW_PAGES_WITH_ID: readonly CrewSettingsPage[] = ["escape", "report"];

// Same grammar as Core Flow valid_id. A locator identifies a read, never an action.
export const isCrewTaskId = (id: string): boolean => /^[A-Za-z0-9_:-][A-Za-z0-9_.:-]{0,95}$/.test(id);

export function routeHref(route: Route): string {
  switch (route.name) {
    case "home":
      return "#/";
    case "thread":
      return `#/threads/${enc(route.id)}`;
    case "new":
      return route.projectId ? `#/new/${enc(route.projectId)}` : "#/new";
    case "project":
      return `#/projects/${enc(route.projectId)}`;
    case "trust":
      return `#/trust/${enc(route.projectId)}${route.threadId ? `/${enc(route.threadId)}` : ""}`;
    case "usage":
      return "#/usage";
    case "skills":
      return route.skill === undefined ? "#/skills" : `#/skills/${enc(route.skill)}`;
    case "tasks":
      return "#/tasks";
    case "crew":
      if (route.task !== undefined) return route.tab === "flow" && isCrewTaskId(route.task) ? `#/crew/flow/task/${enc(route.task)}` : "#/";
      return `#/crew/${route.tab}`;
    case "pulls":
      return route.project !== undefined && route.number !== undefined ? `#/pulls/${enc(route.project)}/${route.number}` : "#/pulls";
    case "settings": {
      const base = route.section ? `#/settings/${route.section}` : "#/settings";
      const path = route.section === "crew" && route.crewPage ? `${base}/${CREW_PAGE_PATH[route.crewPage]}` : base;
      const params = new URLSearchParams();
      if (route.machine !== undefined) params.set("machine", route.machine);
      if (route.selection?.kind === "project_group" || route.selection?.kind === "checkout") params.set("project", route.selection.group_key);
      if (route.selection?.kind === "checkout") params.set("checkout", route.selection.project_id);
      if (route.repo !== undefined && route.crewPage && CREW_PAGES_WITH_REPO.includes(route.crewPage)) params.set("repo", route.repo);
      if (route.from !== undefined && route.crewPage && CREW_PAGES_WITH_ORIGIN.includes(route.crewPage)) params.set("from", route.from);
      if (route.id !== undefined && route.crewPage && CREW_PAGES_WITH_ID.includes(route.crewPage)) params.set("id", route.id);
      if (route.task !== undefined) params.set("task", route.task);
      if (route.task !== undefined && route.edit) params.set("edit", "1");
      return params.size ? `${path}?${params}` : path;
    }
  }
}

/** The Settings › Crew page an address names, with the project or id it carries; an unknown page is the Crew page itself. */
function crewPageOf(second: string | undefined, third: string | undefined, params: URLSearchParams): { crewPage?: CrewSettingsPage; repo?: string; from?: CrewOrigin; id?: string } {
  const both = [second, third].filter(Boolean).join("/");
  // A longer address wins; an unknown sub-page is its parent (`areas/xyz` is Autopilot by area).
  const page = CREW_SETTINGS_PAGES.find((entry) => CREW_PAGE_PATH[entry] === both) ?? CREW_SETTINGS_PAGES.find((entry) => CREW_PAGE_PATH[entry] === second);
  if (!page) return {};
  const origin = CREW_ORIGINS.find((entry) => entry === params.get("from"));
  return {
    crewPage: page,
    ...(CREW_PAGES_WITH_ORIGIN.includes(page) && origin ? { from: origin } : {}),
    ...(CREW_PAGES_WITH_REPO.includes(page) && params.has("repo") ? { repo: params.get("repo") ?? "" } : {}),
    ...(CREW_PAGES_WITH_ID.includes(page) && params.has("id") ? { id: params.get("id") ?? "" } : {}),
  };
}

export function parseRoute(hash: string): Route {
  if (hash.startsWith("#/crew/flow/task")) {
    const segment = /^#\/crew\/flow\/task\/([^/?#]+)$/.exec(hash)?.[1];
    if (!segment) return { name: "home" };
    try {
      const task = decodeURIComponent(segment);
      return isCrewTaskId(task) ? { name: "crew", tab: "flow", task } : { name: "home" };
    } catch { return { name: "home" }; }
  }
  const separator = hash.indexOf("?");
  const path = separator < 0 ? hash : hash.slice(0, separator);
  const search = separator < 0 ? "" : hash.slice(separator + 1);
  const params = new URLSearchParams(search);
  const parts = path
    .replace(/^#\/?/, "")
    .split("/")
    .filter(Boolean)
    .map((part) => {
      try {
        return decodeURIComponent(part);
      } catch {
        return part;
      }
    });
  const [head, first, second, third] = parts;
  switch (head) {
    case "threads":
      return first ? { name: "thread", id: first } : { name: "home" };
    case "new":
      return { name: "new", projectId: first ?? null };
    case "projects":
      return first ? { name: "project", projectId: first } : { name: "home" };
    case "trust":
      return first ? { name: "trust", projectId: first, threadId: second ?? null } : { name: "home" };
    case "usage":
      return { name: "usage" };
    case "skills":
      return first ? { name: "skills", skill: first } : { name: "skills" };
    case "tasks":
      return { name: "tasks" };
    case "crew":
      return { name: "crew", tab: CREW_TABS.find((tab) => tab === first) ?? "approvals" };
    case "pulls": {
      const number = Number(second);
      return first && Number.isInteger(number) && number > 0 ? { name: "pulls", project: first, number } : { name: "pulls" };
    }
    case "settings": {
      // Presence matters: stale and empty IDs remain explicit instead of widening scope.
      const selection: ProjectSettingsSelection | undefined = params.has("checkout")
        ? { kind: "checkout", group_key: params.get("project") ?? "", project_id: params.get("checkout") ?? "" }
        : params.has("project") ? { kind: "project_group", group_key: params.get("project") ?? "" } : undefined;
      return {
        name: "settings",
        section: SETTINGS_SECTIONS.find((section) => section === first) ?? null,
        ...(selection ? { selection } : {}),
        ...(params.has("machine") ? { machine: params.get("machine") ?? "" } : {}),
        ...(first === "crew" ? crewPageOf(second, third, params) : {}),
        ...(params.has("task") ? { task: params.get("task") ?? "", ...(params.get("edit") === "1" ? { edit: true } : {}) } : {}),
      };
    }
    default:
      return { name: "home" };
  }
}

export function navigate(route: Route): void {
  const current = parseRoute(window.location.hash);
  const next = route.name === "settings" && !("selection" in route) && current.name === "settings"
    ? { ...route, selection: current.selection, machine: current.machine } : route;
  goToHash(routeHref(next).slice(1));
}

/**
 * Moves to a hash route. While a modal sheet holds its history entry (`history.state.sheet`), the
 * navigation replaces that entry instead of pushing one, and the sheet closes with it: the stack
 * [list, list + sheet] becomes [list, destination]. Typed addresses and links out can't be seen.
 */
export function goToHash(hash: string): void {
  if (deferNavigation(() => goToHash(hash))) return;
  if ((window.history.state as { sheet?: boolean } | null)?.sheet === true) {
    window.history.replaceState(null, "", `#${hash}`);
    // replaceState says nothing, so tell the router and the sheet.
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    return;
  }
  window.location.hash = hash;
}

/** Swaps the current history entry for a route, so Back skips the screen that only passed the person on. */
export function redirect(route: Route): void {
  window.history.replaceState(null, "", routeHref(route));
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

const subscribe = (notify: () => void) => {
  window.addEventListener("hashchange", notify);
  return () => window.removeEventListener("hashchange", notify);
};

export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, () => navigationSnapshot(window.location.hash));
  return parseRoute(hash);
}
