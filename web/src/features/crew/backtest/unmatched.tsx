import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-unmatched`. */
export function BacktestUnmatched() {
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
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Back to the report</Btn>
            </span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.5fr) minmax(0,.7fr) minmax(0,1fr) minmax(0,1.3fr) minmax(0,1.5fr)" }}>
            <div className="cx-h">Lane</div>
            <div className="cx-h">Findings raised</div>
            <div className="cx-h">Matched an escape</div>
            <div className="cx-h">On a PR with no revert or fix</div>
            <div className="cx-h">On an escaped PR, about something else</div>
            <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
            <div><b>21</b></div>
            <div><b>6</b></div>
            <div><b>12</b></div>
            <div><b>3</b></div>
            <div><b>Correctness · Codex · GPT-5.5</b></div>
            <div><b>17</b></div>
            <div><b>5</b></div>
            <div><b>9</b></div>
            <div><b>3</b></div>
            <div><b>Security · OpenCode · DeepSeek V4</b></div>
            <div><b>9</b></div>
            <div><b>3</b></div>
            <div><b>5</b></div>
            <div><b>1</b></div>
          </div>
          <h5 className="cx-t10h5">Three unmatched findings from Claude Code</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.5fr) minmax(0,2fr) minmax(0,1.6fr)" }}>
            <div className="cx-h">Pull request</div>
            <div className="cx-h">A finding from Claude Code · Sonnet 5.5</div>
            <div className="cx-h">What happened next</div>
            <div><b>#397</b><small>Add a retry to the calendar feed</small></div>
            <div><b>“The retry loop has no upper bound.”</b></div>
            <div><b>No revert or fix in 30 days</b><small>The code has a limit three lines below</small></div>
            <div><b>#409</b><small>Rename the digest settings</small></div>
            <div><b>“The old key is still read in two places.”</b></div>
            <div><b>No revert or fix in 30 days</b><small>Nobody opened an issue</small></div>
            <div><b>#444</b><small>Use next in the auth redirect</small></div>
            <div><b>“An absolute URL in next isn’t checked.”</b></div>
            <div><b>Escaped for another reason</b><small>#444 was fixed for a different problem</small></div>
          </div>
          <div className="cx-fbx">
            <h6>Unmatched isn’t the same as wrong</h6>
            <p>
              A finding no revert or fix backed may have been right and never mattered, or right and nobody looked, or wrong. The backtest can’t tell these apart, so it doesn’t call a finding a false alarm. It counts them, and precision is the share that matched.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
