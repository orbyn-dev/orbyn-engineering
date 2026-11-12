import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-empty`. */
export function BacktestEmpty() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={4} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <span style={{ color: "var(--color-muted)", fontSize: "12.5px" }}>Dashboard ›</span>
            <b>Reviewer backtest</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-docs<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Settings</Btn>
            </span>
          </div>
          <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
            <b>No pull requests with a known outcome yet</b>
            <p>
              orbyn-docs has 3 merged pull requests, and the oldest merged 9 days ago. A backtest replays pull requests that merged at least 30 days ago, so a revert or a fix has had time to happen.
            </p>
            <Btn className="cx-ghost" style={{ height: "28px" }}>Open Settings › Crew › Reviewer backtest</Btn>
          </div>
        </div>
      </div>
    </Frame>
  );
}
