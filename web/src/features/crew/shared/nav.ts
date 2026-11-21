import { createContext, useContext } from "react";
import { routeHref, type Route } from "../../../product/state/route";

/** A place the Crew entry pages send a person to. `project` is the project the page was showing, so the next page opens on it. */
export type CrewPlace =
  | { page: "coverage"; project?: string }
  | { page: "escapes" }
  | { page: "escape"; id: string }
  | { page: "backtest" }
  | { page: "estimate" }
  | { page: "report"; id: string };

/** Where a place lives in the app: a Settings › Crew page, so the browser's Back returns to the page the person came from. */
export function appHref(place: CrewPlace): string {
  const route: Route = { name: "settings", section: "crew", crewPage: place.page };
  if (place.page === "coverage" && place.project !== undefined) return routeHref({ ...route, repo: place.project });
  if (place.page === "escape" || place.page === "report") return routeHref({ ...route, id: place.id });
  return routeHref(route);
}

/**
 * How a screen turns a place into an address. The app's own pages use the default (Settings › Crew); the dev-only Preview
 * route supplies its own, so a screen opened by a frame hash links to the frames and stays in the preview.
 */
export const CrewHref = createContext<(place: CrewPlace) => string>(appHref);

export const useCrewHref = () => useContext(CrewHref);
