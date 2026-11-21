import { TriangleAlert } from "lucide-react";
import { navigate } from "../../product/state/route";
import { Button } from "../../ui/controls";
import { useTrackers } from "./api";
import { TRACKER_NAMES } from "./contract";
import { knownTrackers, trackerLine, trackerMark, trackerStatus } from "./model";
import { StatusChip } from "./TrackerTokens";
import "./trackers.css";

/**
 * The Welcome page's Issue trackers section: each tracker with where its credential stands. The page never takes a token:
 * Add token… and Replace… go to Settings › Source control, where the one token dialog lives. Nothing is drawn until the
 * service reports its trackers.
 */
export function WelcomeTrackers() {
  const { rows } = useTrackers();
  if (rows === null) return null;
  const list = knownTrackers(rows);
  const rejected = list.find((row) => trackerStatus(row) === "rejected");
  const toSettings = () => navigate({ name: "settings", section: "source-control" });
  return (
    <section aria-labelledby="welcome-trackers">
      <h2 id="welcome-trackers">Issue trackers <span className="trk-beta">Beta</span></h2>
      {rejected ? (
        <p className="trk-welcome-warn" role="alert"><TriangleAlert size={14} aria-hidden />{TRACKER_NAMES[rejected.tracker]} didn’t accept the saved token. Replace it or remove it in Settings › Source control.</p>
      ) : null}
      <ul className="trk-welcome" aria-label="Issue trackers">
        {list.map((row) => {
          const status = trackerStatus(row);
          const name = TRACKER_NAMES[row.tracker];
          return (
            <li key={row.tracker}>
              <span className="set-mark" aria-hidden>{trackerMark(row.tracker)}</span>
              <span><b>{name}</b><small>{trackerLine(row)}</small></span>
              <StatusChip status={status} />
              {row.tracker !== "github" && status === "not_connected" ? <Button small aria-label={`Add a token for ${name}`} onClick={toSettings}>Add token…</Button> : null}
              {row.tracker !== "github" && (status === "saved" || status === "rejected" || status === "unreachable") ? <Button small tone="quiet" aria-label={`Replace the token for ${name}`} onClick={toSettings}>Replace…</Button> : null}
            </li>
          );
        })}
      </ul>
      <p className="trk-welcome-note">Issues appear in the left bar once a tracker is connected.</p>
    </section>
  );
}
