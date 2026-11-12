import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-report`. */
export function BacktestReport() {
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
              <Btn className="cx-ghost">Settings</Btn>
              <Btn className="cx-btn">Run again</Btn>
            </span>
          </div>
          <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(3,minmax(0,1fr))" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Pull requests replayed</span>
              <span className="cx-t10b">64</span>
              <span className="cx-t10s">merged, #392 to #455, with a known outcome</span>
              <span className="cx-t10f">Last 180 days, 30 days old or more · 55 had no revert or fix, 9 escaped: 6 reverted, 3 fixed within 30 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes caught by at least one lane or the checks</span>
              <span className="cx-t10b">8 of 9</span>
              <span className="cx-t10s">#452 was caught by none</span>
              <span className="cx-t10f">Counts alone: 9 escapes is a small sample</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">What it used</span>
              <span className="cx-t10b">Per engine</span>
              <span className="cx-t10s">each in its own unit, never added up</span>
              <div className="cx-t10e">
                <span><i>Claude Code</i><b>1.28M tokens</b></span>
                <span><i>Codex</i><b>896k tokens</b></span>
                <span><i>OpenCode</i><b>396k tokens</b></span>
                <span><i>Checks</i><b>No tokens</b></span>
              </div>
              <span className="cx-t10f">64 replays per lane · Sat 7 Nov</span>
            </div>
          </div>
          <h5 className="cx-t10h5">By lane</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.5fr) minmax(0,.7fr) minmax(0,1.2fr) minmax(0,.8fr) minmax(0,1.2fr)" }}>
            <div className="cx-h">Lane</div>
            <div className="cx-h">Replayed</div>
            <div className="cx-h">Escapes it would have caught</div>
            <div className="cx-h">Findings raised</div>
            <div className="cx-h">Findings that matched an escape</div>
            <div><b>Correctness · Claude Code · Sonnet 5.5</b><small>Anthropic</small></div>
            <div><b>64 PRs</b></div>
            <div><b>6 of 9</b><small>recall 67%</small></div>
            <div><b>21</b></div>
            <div><b>6 of 21</b><small>precision 29%</small></div>
            <div><b>Correctness · Codex · GPT-5.5</b><small>OpenAI</small></div>
            <div><b>64 PRs</b></div>
            <div><b>5 of 9</b><small>recall 56%</small></div>
            <div><b>17</b></div>
            <div><b>5 of 17</b><small>precision 29%</small></div>
            <div><b>Security · OpenCode · DeepSeek V4</b><small>DeepSeek · runs on PRs that touch auth/ or keys/</small></div>
            <div><b>22 PRs</b><small>of 64</small></div>
            <div><b>3 of 4</b><small>too few for a rate</small></div>
            <div><b>9</b></div>
            <div><b>3 of 9</b><small>too few for a rate</small></div>
            <div><b>Checks</b><small>The repo’s tests, types and lint</small></div>
            <div><b>64 PRs</b></div>
            <div><b>3 of 9</b><small>recall 33%</small></div>
            <div><b>Failed on 7 PRs</b></div>
            <div><b>3 of 7</b><small>too few for a rate</small></div>
          </div>
          <div className="cx-t14two">
            <div className="cx-fbx">
              <h6>How to read the columns</h6>
              <p>
                Escapes it would have caught: of the 9 pull requests that escaped, how many this lane flagged when replayed. A rate shows from 5 escapes. Findings that matched an escape: of the findings the lane raised, how many pointed at a change that was later reverted or fixed. A rate shows from 10 findings.
              </p>
            </div>
            <div className="cx-fbx">
              <h6>What this tells you, and what it doesn’t</h6>
              <p>
                It replays the review lanes and the checks on pull requests that already merged, as they were at merge time, and compares them with what happened next. It never replays the builder: building isn’t repeatable, so a replay of it would measure nothing. A model’s review can differ from one run to the next, so each replay is one sample, not a promise. Crew makes no claim that one engine, or one vendor, reviews better. This is what happened on this repo, with the sample under every figure.
              </p>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
