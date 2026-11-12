import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-low-sample`. */
export function OracleLowSample() {
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
            <b>Dashboard</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-planner<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>This repo has 5 merges in the last 60 days. A rate shows from 10, so the tiles show the counts they rest on.</span>
          </div>
          <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(2,minmax(0,1fr))" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Merges with an independent key</span>
              <span className="cx-t10b">2 of 5</span>
              <span className="cx-t10s">written by an engine other than the builder</span>
              <span className="cx-t10f">5 merges · last 60 days · too few for a rate</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">The gate on those 2</span>
              <span className="cx-t10b">2 passed</span>
              <span className="cx-t10s">0 strengthened · 0 without the gate · 0 Not measured</span>
              <span className="cx-t10f">2 keys · last 60 days · too few for a rate</span>
            </div>
          </div>
          <div className="cx-fbx">
            <h6>The two keys</h6>
            <div className="cx-fkv">
              <span>Sat 31 Oct</span>
              <b>Retry the feed sync on a timeout · key by Claude Code · Sonnet 5.5 · TypeScript · 12 of 14 mutants killed</b>
              <span>Wed 4 Nov</span>
              <b>Update the calendar feed token · key by Claude Code · Sonnet 5.5 · TypeScript · 9 of 9 mutants killed</b>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
