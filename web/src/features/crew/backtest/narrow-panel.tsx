import { Frame } from "../shared/Frame";
import { PanelShell } from "../shared/PanelShell";

/** Crew Designs v23, frame `backtest-narrow-panel`. */
export function BacktestNarrowPanel() {
  return (
    <Frame panel>
      <PanelShell on="Review">
        <div style={{ padding: "6px 10px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0", flex: 1 }}>
          <div className="cx-t10hd"><b style={{ fontSize: "13px" }}>Reviewer backtest</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>64 pull requests replayed</b>
            <div className="cx-t14r"><span className="cx-apchip">Sat 7 Nov</span></div>
            <small>#392 to #455 · last 180 days · 55 had no revert or fix</small>
            <small>9 escaped: 6 reverted, 3 fixed within 30 days</small>
          </div>
          <div className="cx-t14card">
            <b>Correctness · Claude Code · Sonnet 5.5</b>
            <small>Would have caught 6 of 9 · recall 67%</small>
            <small>6 of 21 findings matched an escape</small>
          </div>
          <div className="cx-t14card">
            <b>Correctness · Codex · GPT-5.5</b>
            <small>Would have caught 5 of 9 · recall 56%</small>
            <small>5 of 17 findings matched an escape</small>
          </div>
          <div className="cx-t14card">
            <b>Security · OpenCode · DeepSeek V4</b>
            <small>Would have caught 3 of 4 · too few for a rate</small>
            <small>3 of 9 findings matched · too few for a rate</small>
          </div>
          <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>
            It replays reviews and checks, never builders. The table, the escapes and the notes are behind Open in Crew.
          </span>
        </div>
      </PanelShell>
    </Frame>
  );
}
