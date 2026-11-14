import { Users } from "lucide-react";
import { routeHref, type Route } from "../../../../product/state/route";
import { api } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import "./flow-live.css";

/**
 * The Crew row of the left bar, after Pull requests (Engineer Designs, the explorer frames): the icon, "Crew" and the Preview
 * label. Drawn only while the service says Crew Preview is on (`crew_preview` in its info); it opens the Crew view on Flow. The list of crews under it is Planned in the design
 * and is not drawn.
 */
export function CrewNavRow({ route }: { route: Route }) {
  // The flag comes from the service's info, which the app already reads at start: `crew_preview` is true only when Crew Preview
  // is on and absent otherwise. The shell never asks a Crew route, which answers 404 (a failed request the browser logs) when it is off.
  const info = useAsync("crew-nav-info", (signal) => api.info(signal), { keep: true });
  if (info.data?.crew_preview !== true) return null;
  return (
    <a className="nav-row" href={routeHref({ name: "crew", tab: "flow" })} aria-current={route.name === "crew" ? "page" : undefined}>
      <Users size={15} aria-hidden />
      <span className="nav-row-label">Crew</span>
      <span className="fv-nav-note">Preview</span>
    </a>
  );
}
