import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-over-budget`. */
export function BacktestOverBudget() {
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
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "660px" }}>
              <h4>{"This backtest is over your budget "}<span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span></h4>
              <p>You chose the last 365 days: 130 pull requests. The estimate for each engine is above its run budget, so Crew hasn’t started. Nothing has run.</p>
              <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.7fr) minmax(0,.8fr) minmax(0,1.1fr) minmax(0,1fr)" }}>
                <div className="cx-h">Lane</div>
                <div className="cx-h">Replays</div>
                <div className="cx-h">Estimate</div>
                <div className="cx-h">Your budget</div>
                <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
                <div><b>130 reviews</b></div>
                <div><b>about 2.6M tokens</b></div>
                <div><b>1.5M tokens</b><small>over by 1.1M tokens</small></div>
                <div><b>Correctness · Codex · GPT-5.5</b></div>
                <div><b>130 reviews</b></div>
                <div><b>about 1.82M tokens</b></div>
                <div><b>1M tokens</b><small>over by 820k tokens</small></div>
                <div><b>Security · OpenCode · DeepSeek V4</b></div>
                <div><b>45 reviews</b></div>
                <div><b>about 810k tokens</b></div>
                <div><b>500k tokens</b><small>over by 310k tokens</small></div>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                <div className="cx-t14opt cx-on">
                  <Ic n="check2" sm />
                  <span>
                    <b>Use the last 180 days instead</b>
                    <small>64 pull requests. It fits all three budgets (about 1.28M tokens, 896k tokens and 396k tokens).</small>
                  </span>
                </div>
                <div className="cx-t14opt">
                  <Ic n="edit" sm />
                  <span>
                    <b>Raise the budgets</b>
                    <small>To 2.6M tokens for Claude Code, 1.82M tokens for Codex and 810k tokens for OpenCode. It changes the setting for every backtest.</small>
                  </span>
                </div>
                <div className="cx-t14opt">
                  <Ic n="minus" sm />
                  <span><b>Run one lane</b><small>Choose the lane to replay. The others show Not run in the report.</small></span>
                </div>
              </div>
              <div className="cx-ra"><Btn className="cx-ghost">Cancel</Btn><Btn className="cx-btn">Use the last 180 days</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
