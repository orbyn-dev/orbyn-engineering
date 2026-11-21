import type { ReactNode } from "react";
import { CrewHref, type CrewPlace } from "./nav";

const q = (id: string) => `?id=${encodeURIComponent(id)}`;

/** The frame a place opens in the Preview route. Only the Preview chunk imports this file, so no frame hash ships in the app. */
function previewHref(place: CrewPlace): string {
  switch (place.page) {
    case "crew":
      return "#/settings/crew";
    case "coverage":
      return "#/preview/crew/capacity-live-coverage";
    case "capacity":
      return "#/preview/crew/capacity-live-settings";
    case "escapes":
      return "#/preview/crew/escape-log";
    case "possible":
      return "#/preview/crew/escape-possible";
    case "escape-settings":
      return "#/preview/crew/escape-settings";
    case "escape":
      return `#/preview/crew/escape-detail${q(place.id)}`;
    case "backtest":
      // The page itself is an app page; the Preview has no frame of it.
      return "#/settings/crew/backtest";
    case "estimate":
      return "#/preview/crew/backtest-estimate";
    case "report":
      return `#/preview/crew/backtest-report${q(place.id)}`;
  }
}

export function PreviewNav({ children }: { children: ReactNode }) {
  return <CrewHref.Provider value={previewHref}>{children}</CrewHref.Provider>;
}
