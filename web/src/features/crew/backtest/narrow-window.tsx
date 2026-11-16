import { Frame } from "../shared/Frame";
import { PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-narrow-window`. */
export function BacktestNarrowWindow() {
  return (
    <Frame width={640}>
      <div className="cx-cwpg">
        <PageHead sub="Dashboard" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t10hd"><b>Reviewer backtest</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>64 pull requests replayed</b>
            <div className="cx-t14r"><span className="cx-apchip">Sat 7 Nov</span></div>
            <small>#392 to #455 · last 180 days · 9 escaped</small>
            <small>It replays reviews and checks, never the builder.</small>
          </div>
          <div className="cx-t14card">
            <b>Correctness · Claude Code · Sonnet 5.5</b>
            <small>Would have caught 6 of 9 · recall 67%</small>
            <small>6 of 21 findings matched an escape · precision 29%</small>
          </div>
          <div className="cx-t14card">
            <b>Correctness · Codex · GPT-5.5</b>
            <small>Would have caught 5 of 9 · recall 56%</small>
            <small>5 of 17 findings matched an escape · precision 29%</small>
          </div>
          <div className="cx-t14card">
            <b>Security · OpenCode · DeepSeek V4</b>
            <small>Would have caught 3 of 4 · too few for a rate</small>
            <small>3 of 9 findings matched · too few for a rate</small>
          </div>
          <div className="cx-t14card">
            <b>Checks</b>
            <small>Would have caught 3 of 9 · recall 33%</small>
            <small>Failed on 7 pull requests, 3 of which escaped</small>
          </div>
        </div>
      </div>
    </Frame>
  );
}
