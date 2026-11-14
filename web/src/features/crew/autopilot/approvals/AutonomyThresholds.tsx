// Settings › Crew › Autopilot by area › Thresholds (Crew Designs v39, frame `autonomy-thresholds`): the breaker's constants, read-only, each Proposed.
// "Earn upward" is drawn in the design as Planned; earning a higher mode is not built, so the row is left out until it is.
import { Info } from "lucide-react";
import { useAsync } from "../../../../lib/useAsync";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { crewApi } from "./api";
import { plural } from "./areas";
import { GUARD_WORKERS } from "./copy";
import { Chip, Facts } from "./parts";

export function AutonomyThresholds() {
  const read = useAsync("crew-thresholds", (signal) => crewApi.thresholds(signal), { keep: true });
  if (read.status === "loading") return <Loading label="Reading the thresholds" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read the thresholds" failure={read.error} onRetry={read.reload} />;
  const t = read.data;
  const mark = t.proposed ? " · Proposed" : "";
  const rows: { title: string; text: string; value: string }[] = [
    { title: "Window", text: "How far back Crew counts merges and reverts for an area.", value: `${plural(t.window_days, "day")}${mark}` },
    { title: "Merges needed", text: "Below this many merges in the window an area says “Not enough data” and nothing changes.", value: `${t.min_merges}${mark}` },
    { title: "Demote when", text: "An area whose ceiling is Full drops to On. A ceiling of On or Off is never changed.", value: `${plural(t.min_merges, "merge")}, ${t.trip_reverts} reverted${mark}` },
    {
      title: "Return when",
      text: `A demoted area goes back to Full once ${t.recovery_days} days have passed since its last revert and ${t.recovery_merges} merges since then were none reverted. Checked at a merge; counts only.`,
      value: `${t.recovery_days} days, ${t.recovery_merges} merges${mark}`,
    },
  ];
  return (
    <section className="ap-set ar" aria-labelledby="ar-title" data-frame="autonomy-thresholds">
      <header className="ap-set-head">
        <h2 id="ar-title">Crew › Autopilot by area › Thresholds</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-lead">These are the defaults. They are read-only here. Engineering sets the values; they are not read from the repo file.</p>
      <div className="ar-set" role="list" aria-label="Thresholds">
        {rows.map((row) => (
          <div key={row.title} className="ar-setrow" role="listitem">
            <div><b>{row.title}</b><small>{row.text}</small></div>
            <span className="ar-val">{row.value}</span>
          </div>
        ))}
      </div>
      <section className="ap-box" aria-label="What counts">
        <h4>What counts</h4>
        <Facts
          rows={[
            { label: "An area", text: "The first two folders of a path, such as web/app or crates/orbyn-core" },
            { label: "A revert", text: "A merge that Crew or you reverted, counted against the area it touched most" },
            { label: "An escape", text: "Not measured" },
            { label: "Effect", text: "While an area is on On, a High risk diff in it waits for you at the merge gate. The risk tier itself is unchanged." },
          ]}
        />
      </section>
      {t.proposed ? <div className="ar-chips"><span className="ar-mk">Proposed: Engineering sets the real values</span></div> : null}
      <p className="ar-note"><Info size={14} aria-hidden /><span>{GUARD_WORKERS}</span></p>
    </section>
  );
}
