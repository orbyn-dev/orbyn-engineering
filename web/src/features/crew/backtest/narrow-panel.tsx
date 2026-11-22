import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";
import { NarrowReportCards } from "./narrow-cards";

/** Crew Designs v95, frame `backtest-narrow-panel`: the report as one run card and one card per lane. */
export function BacktestNarrowPanel() {
  return (
    <Frame panel height={860}>
      <PanelShell on="Review">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-t10hd"><b style={{ fontSize: "13px" }}>Reviewer backtest</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <NarrowReportCards tail="It replays reviews and checks, never builders. The full table is behind Open in Crew." />
        </div>
      </PanelShell>
    </Frame>
  );
}
