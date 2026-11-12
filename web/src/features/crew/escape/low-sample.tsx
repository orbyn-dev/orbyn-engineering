import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-low-sample`. */
export function EscapeLowSample() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr"><span style={{ color: "var(--color-muted)" }}>Dashboard</span><Ic n="right" sm /><b>Escape log</b></div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Escape log</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-planner<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              This repo has 5 merges in the last 60 days. A rate shows from 10 merges, so each tile says what it rests on and the escape is listed below.
            </span>
          </div>
          <div className="cx-t10g">
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes</span>
              <span className="cx-t10b">1 of 5</span>
              <span className="cx-t10s">merges got past review and checks</span>
              <span className="cx-t10f">5 merges · too few for a rate</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Why it got through</span>
              <span className="cx-t10b">1 test gap</span>
              <span className="cx-t10s">and 0 tooling gaps</span>
              <span className="cx-t10f">1 escape · last 60 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Checks from escapes</span>
              <span className="cx-t10b">1 added</span>
              <span className="cx-t10s">and 0 being written</span>
              <span className="cx-t10f">1 escape · last 60 days</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Counted against</span>
              <span className="cx-t10b">Per engine</span>
              <span className="cx-t10s">escapes of the engine’s merges</span>
              <div className="cx-t10e"><span><i>Claude Code</i><b>1 of 4</b></span><span><i>Codex</i><b>0 of 1</b></span></div>
              <span className="cx-t10f">Merges per engine · last 60 days</span>
            </div>
          </div>
          <h5 className="cx-t10h5">The escape<span className="cx-t10r">1 in last 60 days</span></h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,.9fr) minmax(0,1.5fr) minmax(0,.7fr) minmax(0,1.5fr) minmax(0,1fr)" }}>
            <div className="cx-h">When</div>
            <div className="cx-h">What escaped</div>
            <div className="cx-h">Gap</div>
            <div className="cx-h">The check it created</div>
            <div className="cx-h">Counts against</div>
            <div><b>Sat 7 Nov, 10:15</b><small>Found by you · not reverted · merged Fri 6 Nov, 16:40</small></div>
            <div><b>Update the calendar feed token</b><small>The old feed token still worked after it was replaced</small></div>
            <div><span className="cx-apchip">Test gap</span></div>
            <div>
              <b>A replaced feed token is rejected</b>
              <small>tests/feed/token.test.ts</small>
              <span style={{ display: "inline-flex", marginTop: "4px" }}><span className="cx-apchip cx-ok">Added Sat 7 Nov, 11:30</span></span>
            </div>
            <div><b>Claude Code · Sonnet 5.5</b><small>feed/</small></div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
