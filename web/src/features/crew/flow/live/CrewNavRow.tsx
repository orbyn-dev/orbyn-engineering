import { Users } from "lucide-react";
import { routeHref, type Route } from "../../../../product/state/route";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import "./flow-live.css";

/**
 * The Crew row of the left bar, after Pull requests (Engineer Designs, the explorer frames): the icon, "Crew" and the Preview
 * label. Drawn only while Crew Preview is on; it opens the Crew view on Flow. The list of crews under it is Planned in the design
 * and is not drawn.
 */
export function CrewNavRow({ route }: { route: Route }) {
  const on = useCrewPreview();
  if (!on) return null;
  return (
    <a className="nav-row" href={routeHref({ name: "crew", tab: "flow" })} aria-current={route.name === "crew" ? "page" : undefined}>
      <Users size={15} aria-hidden />
      <span className="nav-row-label">Crew</span>
      <span className="fv-nav-note">Preview</span>
    </a>
  );
}
