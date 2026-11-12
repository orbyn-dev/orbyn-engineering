import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-lane-failed`. */
export function BacktestLaneFailed() {
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
            <span className="cx-st cx-err"><i className="cx-sd" />A lane stopped</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Report without Codex</Btn>
              <Btn className="cx-btn">Continue Codex later</Btn>
            </span>
          </div>
          <div className="cx-cwbn cx-red" style={{ margin: "0" }}>
            <Ic n="alert" sm />
            <span>
              <b>The Codex lane stopped after 31 of 64 pull requests.</b>
              {" Codex reached its weekly limit. The other lanes finished. Nothing was changed, and no miss is counted for the 33 pull requests it didn’t replay."}
            </span>
            <span className="cx-acts"><Btn className="cx-ghost">Open Usage</Btn></span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.6fr) minmax(0,.9fr) minmax(0,1.3fr) minmax(0,.9fr) minmax(0,.8fr)" }}>
            <div className="cx-h">Lane</div>
            <div className="cx-h">Replayed</div>
            <div className="cx-h">Escapes it would have caught</div>
            <div className="cx-h">Used</div>
            <div className="cx-h">State</div>
            <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
            <div><b>64 PRs</b></div>
            <div><b>6 of 9</b><small>recall 67%</small></div>
            <div><b>1.28M tokens</b></div>
            <div><span className="cx-st"><i className="cx-sd" />Done</span></div>
            <div><b>Correctness · Codex · GPT-5.5</b></div>
            <div><b>31 of 64 PRs</b></div>
            <div><b>1 of 2</b><small>of the escapes in these · too few for a rate</small></div>
            <div><b>434k tokens</b></div>
            <div><span className="cx-st cx-err"><i className="cx-sd" />Stopped</span></div>
            <div><b>Security · OpenCode · DeepSeek V4</b></div>
            <div><b>22 PRs</b></div>
            <div><b>3 of 4</b><small>too few for a rate</small></div>
            <div><b>396k tokens</b></div>
            <div><span className="cx-st"><i className="cx-sd" />Done</span></div>
            <div><b>Checks</b></div>
            <div><b>64 PRs</b></div>
            <div><b>3 of 9</b><small>recall 33%</small></div>
            <div><b>No tokens</b></div>
            <div><span className="cx-st"><i className="cx-sd" />Done</span></div>
          </div>
          <div className="cx-fbx">
            <h6>What happens to the Codex row</h6>
            <p>
              It stays in the report as Partial, with just the pull requests it replayed. Continue Codex later replays the rest when its weekly limit has reset, and the row is complete again. The other lanes don’t run a second time.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
