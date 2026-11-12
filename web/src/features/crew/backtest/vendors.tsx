import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-vendors`. */
export function BacktestVendors() {
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
          <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(2,minmax(0,1fr))" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Another vendor than the builder</span>
              <span className="cx-t10b">6 of 9</span>
              <span className="cx-t10s">escapes caught, across both correctness lanes</span>
              <span className="cx-t10f">4 of 6 by Claude Code on Codex’s changes, 2 of 3 by Codex on Claude Code’s</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">The same vendor as the builder</span>
              <span className="cx-t10b">5 of 9</span>
              <span className="cx-t10s">escapes caught, across both correctness lanes</span>
              <span className="cx-t10f">2 of 3 by Claude Code on its own changes, 3 of 6 by Codex on its own</span>
            </div>
          </div>
          <h5 className="cx-t10h5">Each reviewer, split by who built the change</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,2fr) minmax(0,1.1fr) minmax(0,1.1fr)" }}>
            <div className="cx-h">Reviewer</div>
            <div className="cx-h">Reviewer’s vendor against the builder’s</div>
            <div className="cx-h">Escapes it would have caught</div>
            <div><b>Claude Code · Sonnet 5.5</b><small>reviewing a change built by Codex</small></div>
            <div><b>Another vendor</b></div>
            <div><b>4 of 6</b><small>too few for a rate</small></div>
            <div><b>Claude Code · Sonnet 5.5</b><small>reviewing a change built by Claude Code</small></div>
            <div><b>The same vendor</b></div>
            <div><b>2 of 3</b><small>too few for a rate</small></div>
            <div><b>Codex · GPT-5.5</b><small>reviewing a change built by Claude Code</small></div>
            <div><b>Another vendor</b></div>
            <div><b>2 of 3</b><small>too few for a rate</small></div>
            <div><b>Codex · GPT-5.5</b><small>reviewing a change built by Codex</small></div>
            <div><b>The same vendor</b></div>
            <div><b>3 of 6</b><small>too few for a rate</small></div>
          </div>
          <div className="cx-fbx">
            <h6>Too few to compare</h6>
            <p>
              6 of the escapes were built by Codex and 3 by Claude Code. A difference of one escape in either group would swap these numbers. Crew makes no claim that a reviewer on another vendor catches more, and the backtest can’t show it on this much history. More history is what could show it.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
