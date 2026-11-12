import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-estimate`. */
export function BacktestEstimate() {
  return (
    <Frame height={760}>
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
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Settings</Btn>
              <Btn className="cx-btn">Run a backtest</Btn>
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>No backtest has run in this repo yet. Choose Run a backtest to replay the lanes on its history.</span>
          </div>
          <div className="cx-fbx"><h6>The last 180 days</h6><p>64 merged pull requests are 30 days old or more. 9 of them were reverted or fixed.</p></div>
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "640px" }}>
              <h4>Run a reviewer backtest</h4>
              <p>
                Crew will replay the review lanes and the checks on 64 merged pull requests in orbyn-engineering (#392 to #455, the last 180 days, 30 days old or more). It never replays the builder, and nothing is merged, reverted or changed.
              </p>
              <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.7fr) minmax(0,.8fr) minmax(0,1.1fr) minmax(0,1fr)" }}>
                <div className="cx-h">Lane</div>
                <div className="cx-h">Replays</div>
                <div className="cx-h">Estimate</div>
                <div className="cx-h">Your budget</div>
                <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
                <div><b>64 reviews</b></div>
                <div><b>about 1.28M tokens</b></div>
                <div><b>1.5M tokens</b><small>within it</small></div>
                <div><b>Correctness · Codex · GPT-5.5</b></div>
                <div><b>64 reviews</b></div>
                <div><b>about 896k tokens</b></div>
                <div><b>1M tokens</b><small>within it</small></div>
                <div><b>Security · OpenCode · DeepSeek V4</b></div>
                <div><b>22 reviews</b></div>
                <div><b>about 396k tokens</b></div>
                <div><b>500k tokens</b><small>within it</small></div>
                <div><b>Checks</b></div>
                <div><b>64 runs</b></div>
                <div><b>No tokens</b></div>
                <div><b>Not needed</b></div>
              </div>
              <p style={{ fontSize: "12px" }}>
                The estimates come from the size of each diff and are not a quote. Each engine counts in its own unit and the amounts are never added up. Codex’s weekly limit is shared with your other work. Crew stops a lane by itself before it passes its budget, and you can stop the whole run at any time.
              </p>
              <div className="cx-ra"><Btn className="cx-ghost">Cancel</Btn><Btn className="cx-btn">Start the backtest</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
