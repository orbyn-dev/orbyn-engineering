import { Ic } from "../shared/parts";
import { CoverageShell } from "./coverage-shell";

/** Crew Designs v77, frame `capacity-coverage-low`: a few merges get the same gaps banner and exact counts as any map, with no "thin" label. */
export function CapacityCoverageLow() {
  return (
    <CoverageShell project="orbyn-planner">
      <div className="cx-cwbn" style={{ margin: "0" }}>
        <Ic n="info" sm />
        <span>
          <b>2 of 5 merges aren’t in the map.</b>
          {" They are from before Wed 4 Nov 2026, had no stored hunks or had no note written. Their lines are Not measured, not counted as read or unread."}
        </span>
      </div>
      <div className="cx-t15map" style={{ gridTemplateColumns: "1fr" }}>
        <div className="cx-t15tile">
          <b className="cx-a">web/app</b>
          <b className="cx-n">260 of 410</b>
          <small>lines no person has reviewed · 63%</small>
          <div className="cx-sp3"><span><i>Reviewed</i>150</span><span><i>In your queue</i>60</span><span><i>Not in a queue</i>200</span></div>
          <small>3 merges</small>
        </div>
      </div>
    </CoverageShell>
  );
}
