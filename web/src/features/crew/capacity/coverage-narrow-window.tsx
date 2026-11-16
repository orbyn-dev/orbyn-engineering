import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-narrow-window`. */
export function CapacityCoverageNarrowWindow() {
  return (
    <Frame width={640}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t10hd"><b>Review coverage</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
            <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
          </div>
          <div className="cx-t14card">
            <b>3,660 agent-written lines · 62% not read</b>
            <small>2,280 lines no person has read. From 24 merges since Mon 12 Oct 2026; 14 earlier merges are Not measured.</small>
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
      </div>
    </Frame>
  );
}
