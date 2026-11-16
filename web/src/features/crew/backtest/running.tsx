import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-running`. */
export function BacktestRunning() {
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
            <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Running</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Stop the backtest</Btn>
            </span>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14col">
              <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.7fr) minmax(0,1fr) minmax(0,1fr)" }}>
                <div className="cx-h">Lane</div>
                <div className="cx-h">Progress</div>
                <div className="cx-h">Used</div>
                <div><b>Correctness · Claude Code · Sonnet 5.5</b></div>
                <div><b>23 of 64</b><small>reviews replayed</small></div>
                <div><b>460k tokens</b><small>of 1.5M tokens</small></div>
                <div><b>Correctness · Codex · GPT-5.5</b></div>
                <div><b>19 of 64</b><small>reviews replayed</small></div>
                <div><b>266k tokens</b><small>of 1M tokens</small></div>
                <div><b>Security · OpenCode · DeepSeek V4</b></div>
                <div><b>8 of 22</b><small>reviews replayed</small></div>
                <div><b>144k tokens</b><small>of 500k tokens</small></div>
                <div><b>Checks</b></div>
                <div><b>64 of 64</b><small>runs replayed</small></div>
                <div><b>No tokens</b></div>
              </div>
            </div>
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Sat 7 Nov, as of 16:31</span>
              </div>
              <div>
                <span className="cx-t">16:02</span>
                <Ic n="refresh" sm />
                <span>
                  <b>Replay started</b>
                  <small>
                    64 merged pull requests in orbyn-engineering, #392 to #455, starting with the oldest. Each review sees the diff and the checks as they were at merge time, never the later revert or fix.
                  </small>
                </span>
              </div>
              <div>
                <span className="cx-t">16:09</span>
                <Ic n="ok" sm />
                <span><b>Checks replayed</b><small>The repo’s tests, types and lint ran on all 64 merge states.</small></span>
              </div>
              <div>
                <span className="cx-t">16:31</span>
                <Ic n="refresh" sm />
                <span>
                  <b>Reviews running</b>
                  <small>Claude Code is at 23 of 64, Codex at 19 of 64, OpenCode at 8 of 22. It will finish by itself; nothing needs you.</small>
                </span>
              </div>
            </div>
          </div>
          <div className="cx-fbx">
            <h6>No result is shown until it has finished</h6>
            <p>
              The counts above are progress. A result on part of the history can look better or worse than the whole. Crew shows the report when every lane is done, or the part that was replayed, labelled Partial, if you stop it.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
