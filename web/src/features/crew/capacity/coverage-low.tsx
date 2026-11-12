import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-coverage-low`. */
export function CapacityCoverageLow() {
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
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-planner<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={0} items={["Since provenance began"]} />
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>This map rests on 3 merges.</b>
              {" Crew started recording on Wed 4 Nov, so 14 earlier merges are Not measured. The counts below are exact for those 3."}
            </span>
          </div>
          <div className="cx-t15map" style={{ gridTemplateColumns: "1fr" }}>
            <div className="cx-t15tile">
              <b className="cx-a">web/</b>
              <b className="cx-n">260 of 410</b>
              <small>lines no person has reviewed · 63%</small>
              <div className="cx-sp3"><span><i>Reviewed</i>150</span><span><i>In your queue</i>60</span><span><i>Not in a queue</i>200</span></div>
              <small>3 merges</small>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
