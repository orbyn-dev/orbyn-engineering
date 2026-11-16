import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";

/** Crew Designs v21, frame `capacity-coverage-narrow-panel`. */
export function CapacityCoverageNarrowPanel() {
  return (
    <Frame panel>
      <PanelShell on="Review">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-t10hd"><b style={{ fontSize: "13px" }}>Review coverage</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>3,660 agent-written lines · 62% not read</b>
            <small>2,280 of 3,660 lines no person has read. From 24 merges since Mon 12 Oct 2026; 14 earlier merges are Not measured.</small>
          </div>
          <div className="cx-t14card">
            <b>auth/</b>
            <small>510 of 1,420 not read · 36%</small>
            <small>Reviewed 910 · In your queue 310 · Not in a queue 200</small>
            <small>9 merges</small>
          </div>
          <div className="cx-t14card">
            <b>docs/</b>
            <small>940 of 1,060 not read · 89%</small>
            <small>Reviewed 120 · In your queue 0 · Not in a queue 940</small>
            <small>9 merges</small>
          </div>
          <div className="cx-t14card">
            <b>web/</b>
            <small>830 of 1,180 not read · 70%</small>
            <small>Reviewed 350 · In your queue 80 · Not in a queue 750</small>
            <small>6 merges</small>
          </div>
        </div>
      </PanelShell>
    </Frame>
  );
}
