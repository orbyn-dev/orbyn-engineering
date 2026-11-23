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
  "browser",
  "crew",
  "secrets",
  "maintenance",
  "labs",
  "about",
  "archive",
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

/** The pages inside a Settings page, by the section that holds them: `#/settings/appearance/theme-editor`. */
const SETTINGS_SUBPAGES = new Map<SettingsSection, readonly string[]>([["appearance", ["theme-editor"]]]);
export type SettingsSubPage = "theme-editor";
/** What a row key in an address may be: lower-case words joined by hyphens, short. Anything else names no row. */
const ROW_KEY = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/** The Crew screens built so far; the rest join as they land. */
export const CREW_TABS = ["approvals", "trust", "background", "inbox", "flow", "board", "map", "lanes"] as const;
export type CrewTab = (typeof CREW_TABS)[number];

export const CREW_SETTINGS_PAGES = ["areas", "thresholds", "capacity", "coverage", "dashboard", "escapes", "possible", "escape-settings", "escape", "backtest", "estimate", "report", "lessons", "lesson", "lesson-add"] as const;
export type CrewSettingsPage = (typeof CREW_SETTINGS_PAGES)[number];

/** Where the coverage map was opened from. It comes from the address the link carried, never from history: no origin, no way back drawn. */
export const CREW_ORIGINS = ["capacity", "escapes"] as const;
export type CrewOrigin = (typeof CREW_ORIGINS)[number];

export type Route =
  | { name: "home" }
  /** An address that names no screen: the page says so instead of drawing another page's fallback. */
  | { name: "not-found"; path: string }
  | { name: "thread"; id: string }
  | { name: "new"; projectId: string | null }
  /** A project with no threads: the empty screen, or the composer once it has some. */
  | { name: "project"; projectId: string }
  | { name: "trust"; projectId: string; threadId: string | null }
  | {
      name: "settings";
      section: SettingsSection | null;
      /** A page inside the section (the Theme editor in Appearance). */
      sub?: SettingsSubPage;
      /** A row on the page the address names (`#/settings/chat/search-engine`): the page's own row key, never a row id. Settings looks it up in its row list and ignores one it does not have. */
      row?: string;
      /** The page name an address carried that Settings does not have: General opens with a note. Never written back to an address. */
      unknown?: string;
      selection?: ProjectSettingsSelection;
      machine?: string;
      task?: string;
      /** With `task`: open the editor instead of the task page. */
      edit?: boolean;
      /**
       * Settings › Crew › Autopilot by area (and its thresholds), for the repository `repo` (the first one when absent), or a
       * page that picks its own project: Review capacity and its coverage map, the Crew dashboard (`repo` names the project each opens for), the escape log and one escape (`id`), the reviewer backtest, its estimate and one run's report (`id`), and the lessons list, one lesson (`id`) and the add page.
       */
      crewPage?: CrewSettingsPage;
      repo?: string;
      /** With the coverage page: the page whose link opened it, so it can offer the way back. */
      from?: CrewOrigin;
      /** The escape, backtest run or lesson an `escape`, `report` or `lesson` page names. */
      id?: string;
    }
  | { name: "usage" }
  /** The Welcome page: open or clone a project, connect a tracker, pick the engine, go back to a project. */
  | { name: "welcome" }
  | { name: "skills"; skill?: string }
  | { name: "tasks" }
  /** Crew Preview: drawn only while the service says Crew is on. */
  | { name: "crew"; tab: CrewTab; task?: string }
  /** The list, or one pull request of one project. */
  | { name: "pulls"; project?: string; number?: number }
  /** The issues of the connected trackers, or one issue by its tracker, scope and key (a key is unique only within a scope). Drawn only while the service reports issues. */
  | { name: "issues"; tracker?: string; scope?: string; key?: string };

const enc = encodeURIComponent;

/** Where each Settings › Crew page lives under `#/settings/crew/`. */
const CREW_PAGE_PATH: Record<CrewSettingsPage, string> = {
  areas: "areas",
  thresholds: "areas/thresholds",
  capacity: "capacity",
  coverage: "capacity/coverage",
  dashboard: "dashboard",
  escapes: "escapes",
  possible: "escapes/possible",
  "escape-settings": "escapes/settings",
  escape: "escapes/detail",
  backtest: "backtest",
  estimate: "backtest/estimate",
  report: "backtest/report",
  lessons: "lessons",
  lesson: "lessons/lesson",
  "lesson-add": "lessons/add",
};
const CREW_PAGES_WITH_REPO: readonly CrewSettingsPage[] = ["areas", "thresholds", "capacity", "coverage", "dashboard", "escapes", "possible", "lessons", "lesson", "lesson-add"];
const CREW_PAGES_WITH_ORIGIN: readonly CrewSettingsPage[] = ["coverage"];
const CREW_PAGES_WITH_ID: readonly CrewSettingsPage[] = ["escape", "report", "lesson"];

// Same grammar as Core Flow valid_id. A locator identifies a read, never an action.
export const isCrewTaskId = (id: string): boolean => /^[A-Za-z0-9_:-][A-Za-z0-9_.:-]{0,95}$/.test(id);

export function routeHref(route: Route): string {
  switch (route.name) {
    case "home":
      return "#/";
    case "not-found":
      // The address as it arrived: a not-found page never rewrites what the person asked for.
      return route.path ? `#/${route.path}` : "#/";
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
    case "welcome":
      return "#/welcome";
    case "skills":
      return route.skill === undefined ? "#/skills" : `#/skills/${enc(route.skill)}`;
    case "tasks":
      return "#/tasks";
    case "crew":
      if (route.task !== undefined) return route.tab === "flow" && isCrewTaskId(route.task) ? `#/crew/flow/task/${enc(route.task)}` : "#/";
      return `#/crew/${route.tab}`;
    case "pulls":
      return route.project !== undefined && route.number !== undefined ? `#/pulls/${enc(route.project)}/${route.number}` : "#/pulls";
    case "issues":
      return route.tracker !== undefined && route.scope !== undefined && route.key !== undefined ? `#/issues/${enc(route.tracker)}/${enc(route.scope)}/${enc(route.key)}` : "#/issues";
    case "settings": {
      const base = route.section ? `#/settings/${route.section}` : "#/settings";
      const path = route.section === "crew" && route.crewPage ? `${base}/${CREW_PAGE_PATH[route.crewPage]}`
        : route.section && route.sub && SETTINGS_SUBPAGES.get(route.section)?.includes(route.sub) ? `${base}/${route.sub}`
        : route.section && route.section !== "crew" && route.row && ROW_KEY.test(route.row) ? `${base}/${route.row}` : base;
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

/** A segment as typed: a malformed escape names nothing rather than throwing. */
function segmentOf(part: string): string {
  try {
    return decodeURIComponent(part);
  } catch {
    return part;
  }
}

export function parseRoute(hash: string): Route {
  const separator = hash.indexOf("?");
  const path = separator < 0 ? hash : hash.slice(0, separator);
  const search = separator < 0 ? "" : hash.slice(separator + 1);
  const params = new URLSearchParams(search);
  /** The address as typed, without `#/`: a path that names no screen still has one to keep and show. */
  const address = path.replace(/^#\/?/, "");
  if (hash.startsWith("#/crew/flow/task")) {
    const segment = /^#\/crew\/flow\/task\/([^/?#]+)$/.exec(hash)?.[1];
    const task = segment === undefined ? null : segmentOf(segment);
    return task !== null && isCrewTaskId(task) ? { name: "crew", tab: "flow", task } : { name: "not-found", path: address };
  }
  const parts = address
    .split("/")
    .filter(Boolean)
    .map(segmentOf);
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
    case "welcome":
      return { name: "welcome" };
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
    case "issues":
      return first && second && third ? { name: "issues", tracker: first, scope: second, key: third } : { name: "issues" };
    case "settings": {
      // Presence matters: stale and empty IDs remain explicit instead of widening scope.
      const selection: ProjectSettingsSelection | undefined = params.has("checkout")
        ? { kind: "checkout", group_key: params.get("project") ?? "", project_id: params.get("checkout") ?? "" }
        : params.has("project") ? { kind: "project_group", group_key: params.get("project") ?? "" } : undefined;
      const section = SETTINGS_SECTIONS.find((entry) => entry === first) ?? null;
      const sub = section && second ? SETTINGS_SUBPAGES.get(section)?.find((entry) => entry === second) : undefined;
      const row = section && section !== "crew" && !sub && second && ROW_KEY.test(second) ? second : undefined;
      return {
        name: "settings",
        section,
        ...(sub ? { sub: sub as SettingsSubPage } : {}),
        ...(row ? { row } : {}),
        ...(first && !section ? { unknown: first } : {}),
        ...(selection ? { selection } : {}),
        ...(params.has("machine") ? { machine: params.get("machine") ?? "" } : {}),
        ...(first === "crew" ? crewPageOf(second, third, params) : {}),
        ...(params.has("task") ? { task: params.get("task") ?? "", ...(params.get("edit") === "1" ? { edit: true } : {}) } : {}),
      };
    }
    default:
      // A path that matches no screen is its own route: it keeps the address and says not found.
      return address ? { name: "not-found", path: address } : { name: "home" };
  }
}

export function navigate(route: Route): void {
  const current = parseRoute(window.location.hash);
  const next = route.name === "settings" && !("selection" in route) && current.name === "settings"
    ? { ...route, selection: current.selection, machine: current.machine } : route;
  goToHash(routeHref(next).slice(1));
}

/**
 * Settings covers the page under it rather than standing beside it, so it keeps exactly one history entry,
 * marked here: opening it pushes that entry, a move inside Settings replaces it, and closing it goes back.
 * Back therefore never walks back into Settings, and closing adds no entry to walk back out of.
 */
const SETTINGS_ENTRY = "orbynSettings";

const holdsSettingsEntry = (state: unknown): boolean =>
  typeof state === "object" && state !== null && (state as Record<string, unknown>)[SETTINGS_ENTRY] === true;

/**
 * Moves to a hash route. While a modal sheet holds its history entry (`history.state.sheet`), the
 * navigation replaces that entry instead of pushing one, and the sheet closes with it: the stack
 * [list, list + sheet] becomes [list, destination]. Typed addresses and links out can't be seen.
 */
export function goToHash(hash: string): void {
  if (deferNavigation(() => goToHash(hash))) return;
  const state = window.history.state as Record<string, unknown> | null;
  const sheet = state?.sheet === true;
  if (parseRoute(`#${hash}`).name === "settings") {
    const held = holdsSettingsEntry(state);
    if (held) {
      // A move inside Settings keeps the entry the page underneath will find again.
      window.history.replaceState({ ...(state ?? {}), [SETTINGS_ENTRY]: true }, "", `#${hash}`);
    } else {
      // One fresh entry either way: a page under Settings is replaced, and Settings itself arrives with
      // no trail of the page's own, so the navigation reads as a new place rather than a Back step.
      if (sheet) window.history.replaceState({ [SETTINGS_ENTRY]: true }, "", `#${hash}`);
      else window.history.pushState({ [SETTINGS_ENTRY]: true }, "", `#${hash}`);
    }
    // pushState and replaceState say nothing, so tell the router.
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    return;
  }
  if (sheet) {
    window.history.replaceState(null, "", `#${hash}`);
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    return;
  }
  window.location.hash = hash;
}

/**
 * Closes Settings without adding a history entry. When Settings holds the entry it pushed, Back is the way
 * out and the page underneath is already there. An address that arrived on its own (typed, or reloaded with
 * nothing under it) swaps the entry for the page instead, so closing never leaves an entry to back out of.
 */
export function closeSettings(underneath: Route): void {
  if (holdsSettingsEntry(window.history.state)) window.history.back();
  else redirect(underneath);
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
