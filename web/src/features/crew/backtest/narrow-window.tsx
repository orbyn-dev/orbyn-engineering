import { Frame } from "../shared/Frame";
import { PageHead, Seg } from "../shared/parts";
import { NarrowReportCards } from "./narrow-cards";

/** Crew Designs v95, frame `backtest-narrow-window`: the report as one run card and one card per lane. */
export function BacktestNarrowWindow() {
  return (
    <Frame width={640} height={900}>
      <div className="cx-cwpg">
        <PageHead sub="Dashboard" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t10hd"><b>Reviewer backtest</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <NarrowReportCards tail="It replays reviews and checks, never the builder." />
        </div>
      </div>
    </Frame>
  );
}
