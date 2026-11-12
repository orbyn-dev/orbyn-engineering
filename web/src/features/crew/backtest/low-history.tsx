import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-low-history`. */
export function BacktestLowHistory() {
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
            <span className="cx-apchip">Sat 7 Nov</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-planner<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Settings</Btn>
              <Btn className="cx-btn">Run again</Btn>
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>This repo has 14 pull requests with a known outcome and 2 escapes.</b>
              {" A recall rate shows from 5 escapes and a precision rate from 10 findings, so the table shows the counts it rests on."}
            </span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.5fr) minmax(0,.7fr) minmax(0,1.2fr) minmax(0,.8fr) minmax(0,1.2fr)" }}>
            <div className="cx-h">Lane</div>
            <div className="cx-h">Replayed</div>
            <div className="cx-h">Escapes it would have caught</div>
            <div className="cx-h">Findings raised</div>
            <div className="cx-h">Findings that matched an escape</div>
            <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
            <div><b>14 PRs</b></div>
            <div><b>1 of 2</b><small>too few for a rate</small></div>
            <div><b>5</b></div>
            <div><b>1 of 5</b><small>too few for a rate</small></div>
            <div><b>Correctness · Codex · GPT-5.5</b></div>
            <div><b>14 PRs</b></div>
            <div><b>2 of 2</b><small>too few for a rate</small></div>
            <div><b>4</b></div>
            <div><b>2 of 4</b><small>too few for a rate</small></div>
            <div><b>Checks</b></div>
            <div><b>14 PRs</b></div>
            <div><b>0 of 2</b><small>too few for a rate</small></div>
            <div><b>Failed on 1 PR</b></div>
            <div><b>0 of 1</b><small>too few for a rate</small></div>
          </div>
          <div className="cx-fbx">
            <h6>The two escapes</h6>
            <div className="cx-fkv">
              <span>#118</span>
              <b>Update the calendar feed token · reverted · caught by Codex, missed by Claude Code and the checks</b>
              <span>#127</span>
              <b>Retry the feed sync on a timeout · fixed · caught by Claude Code and Codex</b>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
