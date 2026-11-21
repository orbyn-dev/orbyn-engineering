import { routeHref } from "../../../product/state/route";
import { Loading } from "../../../product/ui/states";
import type { AutopilotAreaEvent } from "../../../generated/AutopilotAreaEvent";
import { modeName, plural as pluralOf, rowView, stamp } from "../autopilot/approvals/areas";
import type { DashboardClient } from "./contract";
import { Planned, WHY } from "./parts";
import { text } from "./model";
import { useAreaTrail, type AreaTrail } from "./useDashboard";

const GRID = "minmax(0,1.2fr) minmax(0,1.8fr) minmax(0,1.3fr) minmax(0,.9fr)";
const MAX_CHANGES = 12;

const modeOf = (mode: string | null | undefined) => (mode === "off" || mode === "on" || mode === "full" ? modeName(mode) : "unknown");

/** What a trail row says happened, in the words of the table above it. */
function changeOf(event: AutopilotAreaEvent): { change: string; why: string } {
  const record = event.record;
  switch (event.kind) {
    case "demoted":
      return { change: `${modeOf(event.from_mode)} → ${modeOf(event.to_mode)}`, why: `${pluralOf(record.reverts, "revert")} in ${record.window_days} days, in ${pluralOf(record.merges, "merge")}` };
    case "restored":
      return { change: `${modeOf(event.from_mode)} → ${modeOf(event.to_mode)}`, why: "Returned to your ceiling at a merge: the way back was met" };
    case "recovery_reset":
      return { change: "No change", why: "A new revert started the way back again" };
    case "override_set":
      return { change: `${modeOf(event.from_mode)} → ${modeOf(event.to_mode)}`, why: "You chose it" };
    case "override_cleared":
      return { change: "Follows your ceiling", why: "You cleared your choice" };
    default:
      return { change: "Changed", why: "Recorded by Crew" };
  }
}

function View({ trail, days, id }: { trail: AreaTrail; days: number; id: string }) {
  const { areas, limits } = trail;
  const rows = Array.isArray(areas.rows) ? areas.rows.slice(0, 50) : [];
  const since = areas.as_of - days * 86_400;
  const changes = trail.events.filter((one) => one.event.at >= since).sort((a, b) => b.event.at - a.event.at).slice(0, MAX_CHANGES);
  return (
    <>
      <h5 className="cx-t10h5">
        Autopilot by area
        <span className="cx-rtag cx-lo">Repo ceiling: {modeName(areas.ceiling)}</span>
        <a className="cx-t10r" href={routeHref({ name: "settings", section: "crew", crewPage: "areas", repo: id })} style={{ color: "inherit" }}>Open Autopilot by area</a>
      </h5>
      {rows.length === 0 ? (
        <small className="cx-t10f">No area has a merge in the window yet, so there is no mode to show.</small>
      ) : (
        <div className="cx-aptb" style={{ gridTemplateColumns: GRID }}>
          {["Area", "Now", "Reverted", "Escaped"].map((name) => <div key={name} className="cx-h">{name}</div>)}
          {rows.map((row) => {
            const view = rowView(row, null, limits, areas.ceiling);
            return (
              <div key={row.area_key} style={{ display: "contents" }} data-area-state={row.state}>
                <div><b>{text(row.area)}</b></div>
                <div><b>{modeName(row.now)}</b><small>{text(view.why, 120)}</small></div>
                <div>
                  <b>{view.measured ? view.reverted : "Not measured"}</b>
                  <small>{view.measured ? `reverted · ${view.days} · ${view.merges}` : `${view.merges}, below the ${limits.min_merges} needed`}</small>
                </div>
                <div><b>Not measured</b></div>
              </div>
            );
          })}
        </div>
      )}
      <h5 className="cx-t10h5">Changes of mode<span className="cx-t10r" style={{ textDecoration: "none" }}>Last {days} days</span></h5>
      {changes.length === 0 ? (
        <small className="cx-t10f">No area changed its mode in the last {days} days.</small>
      ) : (
        <div className="cx-t13tl" data-testid="mode-changes">
          <div className="cx-h"><span>When</span><span>Area</span><span>Change</span><span>Why</span></div>
          {changes.map(({ area, event }) => {
            const said = changeOf(event);
            return (
              <div key={`${area}-${event.id}`}>
                <span>{stamp(event.at)}</span>
                <b>{text(area)}</b>
                <span>{said.change}</span>
                <span>{said.why}</span>
              </div>
            );
          })}
        </div>
      )}
      <p className="cx-t10f" style={{ margin: 0 }}>
        Earning a higher mode <Planned why={WHY.earning} /> Escapes are not measured, so that column says so. Thresholds are {limits.proposed ? "Proposed defaults that Engineering sets" : "Engineering’s"}.
      </p>
    </>
  );
}

/** The mode each area is on now and the changes with their reasons: the reads of Autopilot by area, shown beside the record. A failure here leaves the page above it standing. */
export function AreaModes({ client, project, days }: { client: DashboardClient; project: string; days: number }) {
  const read = useAreaTrail(client, project);
  if (read.status === "loading") return <Loading label="Reading Autopilot by area" rows={2} />;
  if (read.status === "error") {
    return (
      <div className="cx-cwbn" style={{ margin: 0 }} role="status">
        <span>Couldn’t read Autopilot by area. Nothing was changed.</span>
        <span className="cx-acts"><button type="button" className="cx-btn" onClick={() => void read.reload()}>Try again</button></span>
      </div>
    );
  }
  return <View trail={read.data} days={days} id={project} />;
}
