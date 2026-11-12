import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-stopped`. */
export function BacktestStopped() {
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
            <span className="cx-apchip">Partial</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Delete the partial result</Btn>
              <Btn className="cx-btn">Continue from #432</Btn>
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>You stopped this backtest at 40 of 64 pull requests.</b>
              {" The counts below are for #392 to #431 and no others. They are exact for those and say little about the rest."}
            </span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.6fr) minmax(0,.9fr) minmax(0,1.5fr) minmax(0,.9fr)" }}>
            <div className="cx-h">Lane</div>
            <div className="cx-h">Replayed</div>
            <div className="cx-h">Escapes it would have caught (of the 4 in these)</div>
            <div className="cx-h">Used</div>
            <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
            <div><b>40 of 64 PRs</b></div>
            <div><b>3 of 4</b><small>too few for a rate</small></div>
            <div><b>800k tokens</b></div>
            <div><b>Correctness · Codex · GPT-5.5</b></div>
            <div><b>40 of 64 PRs</b></div>
            <div><b>3 of 4</b><small>too few for a rate</small></div>
            <div><b>560k tokens</b></div>
            <div><b>Security · OpenCode · DeepSeek V4</b></div>
            <div><b>14 of 22 PRs</b></div>
            <div><b>1 of 2</b><small>too few for a rate</small></div>
            <div><b>252k tokens</b></div>
            <div><b>Checks</b></div>
            <div><b>40 of 64 PRs</b></div>
            <div><b>1 of 4</b><small>too few for a rate</small></div>
            <div><b>No tokens</b></div>
          </div>
          <div className="cx-fbx">
            <h6>Why there is no rate</h6>
            <p>
              A recall rate needs 5 escapes, and the 40 pull requests replayed hold 4. Crew shows the counts and leaves the rate out. Continuing replays the other 24 pull requests and the full report replaces this one.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
