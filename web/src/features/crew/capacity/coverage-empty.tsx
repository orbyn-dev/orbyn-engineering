import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-empty`. */
export function CapacityCoverageEmpty() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr"><span style={{ color: "var(--color-muted)" }}>Dashboard</span><Ic n="right" sm /><b>Review coverage</b></div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Review coverage</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-docs<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={0} items={["Since provenance began"]} />
            </span>
          </div>
          <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
            <b>No agent-written lines are recorded yet</b>
            <p>
              Crew records which lines an agent wrote when it merges. Nothing has merged in orbyn-docs since recording began, so there is nothing to count. The map appears once a merge is recorded.
            </p>
            <Btn className="cx-ghost" style={{ height: "28px" }}>Open the Review queue</Btn>
          </div>
        </div>
      </div>
    </Frame>
  );
}
