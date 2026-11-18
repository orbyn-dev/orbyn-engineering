import { Btn } from "../shared/parts";
import { CoverageShell } from "./coverage-shell";
import { Planned, WHY } from "./Planned";

/** Crew Designs v77, frame `capacity-coverage-empty`: recording has begun and no merge since then has lines to count. */
export function CapacityCoverageEmpty() {
  return (
    <CoverageShell project="orbyn-docs">
      <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
        <b>No agent-written lines are recorded yet</b>
        <p>Crew records which lines an agent wrote when it merges. No merge in orbyn-docs since recording began has lines to count, so there is nothing to show.</p>
        <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <Planned why={WHY.queue} style={{ marginLeft: "0" }} />
          <Btn className="cx-ghost" style={{ height: "28px" }}>Open the Review queue</Btn>
        </span>
      </div>
    </CoverageShell>
  );
}
